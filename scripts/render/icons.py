"""Headless Blender ability-icon renderer for Antediluvia.

Run:  blender --background --python scripts/render/icons.py
Outputs 128x128 PNGs to assets/sprites/icons/<ability>.png

Each icon is a small procedural 3D scene (primitives + emission/PBR
materials) lit consistently and rendered with Cycles-free EEVEE so the
whole set takes seconds and reads as one visual family: dark vignette
background, rim-lit subject, class-colored glow.
"""
import bpy
import math
import os
import sys

OUT_DIR = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "assets", "sprites", "icons"))
SIZE = 128


def reset_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    sc = bpy.context.scene
    sc.render.resolution_x = SIZE
    sc.render.resolution_y = SIZE
    sc.render.film_transparent = False
    # EEVEE (Blender 4.2+: EEVEE Next id is BLENDER_EEVEE_NEXT)
    try:
        sc.render.engine = "BLENDER_EEVEE_NEXT"
    except TypeError:
        sc.render.engine = "BLENDER_EEVEE"
    # World: near-black with a faint warm tint so icons match the HUD panel.
    world = bpy.data.worlds.new("w")
    world.use_nodes = True
    bg = world.node_tree.nodes["Background"]
    bg.inputs[0].default_value = (0.010, 0.010, 0.016, 1.0)
    bg.inputs[1].default_value = 1.0
    sc.world = world
    # Camera: straight-on, slight tilt for depth.
    cam_data = bpy.data.cameras.new("cam")
    cam = bpy.data.objects.new("cam", cam_data)
    sc.collection.objects.link(cam)
    cam.location = (0.0, -4.2, 0.9)
    cam.rotation_euler = (math.radians(78), 0, 0)
    sc.camera = cam
    # Key + rim lights shared by every icon.
    key = bpy.data.lights.new("key", type="AREA")
    key.energy = 400
    key.size = 3
    ko = bpy.data.objects.new("key", key)
    ko.location = (2.2, -2.5, 2.5)
    ko.rotation_euler = (math.radians(50), 0, math.radians(35))
    sc.collection.objects.link(ko)
    rim = bpy.data.lights.new("rim", type="AREA")
    rim.energy = 250
    rim.size = 2
    rim.color = (0.7, 0.8, 1.0)
    ro = bpy.data.objects.new("rim", rim)
    ro.location = (-2.0, 2.0, 1.5)
    ro.rotation_euler = (math.radians(-60), 0, math.radians(-140))
    sc.collection.objects.link(ro)


def mat(name, base, metal=0.0, rough=0.4, emit=None, emit_str=0.0):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = m.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*base, 1.0)
    bsdf.inputs["Metallic"].default_value = metal
    bsdf.inputs["Roughness"].default_value = rough
    if emit is not None:
        bsdf.inputs["Emission Color"].default_value = (*emit, 1.0)
        bsdf.inputs["Emission Strength"].default_value = emit_str
    return m


def add(mesh_op, m, loc=(0, 0, 0), rot=(0, 0, 0), scale=(1, 1, 1), **kw):
    mesh_op(location=loc, rotation=(math.radians(rot[0]), math.radians(rot[1]), math.radians(rot[2])), **kw)
    ob = bpy.context.active_object
    ob.scale = scale
    ob.data.materials.append(m)
    return ob


STEEL = lambda: mat("steel", (0.55, 0.55, 0.60), metal=1.0, rough=0.25)
GOLD = lambda: mat("gold", (0.85, 0.62, 0.15), metal=1.0, rough=0.35)
WOOD = lambda: mat("wood", (0.28, 0.16, 0.07), rough=0.8)
FIRE = lambda: mat("fire", (1.0, 0.25, 0.02), emit=(1.0, 0.30, 0.02), emit_str=2.2)
FROST = lambda: mat("frost", (0.35, 0.65, 1.0), emit=(0.25, 0.55, 1.0), emit_str=1.8)
HOLY = lambda: mat("holy", (1.0, 0.9, 0.45), emit=(1.0, 0.85, 0.35), emit_str=2.0)
SHADOW = lambda: mat("shadow", (0.4, 0.2, 0.6), emit=(0.5, 0.2, 0.8), emit_str=1.5)
BLOOD = lambda: mat("blood", (0.6, 0.05, 0.05), rough=0.5, emit=(0.8, 0.1, 0.1), emit_str=1.0)
LEAF = lambda: mat("leaf", (0.15, 0.5, 0.15), emit=(0.2, 0.8, 0.2), emit_str=1.2)


def rotate_objects(objs, rot, pivot=(0, 0, 0.3)):
    """Rotate the given objects as one group about `pivot` (matrix math —
    bpy.ops.transform is unreliable in --background)."""
    from mathutils import Matrix, Vector
    piv = Vector(pivot)
    m = (Matrix.Translation(piv)
         @ Matrix.Rotation(math.radians(rot[0]), 4, "X")
         @ Matrix.Rotation(math.radians(rot[1]), 4, "Y")
         @ Matrix.Rotation(math.radians(rot[2]), 4, "Z")
         @ Matrix.Translation(-piv))
    for ob in objs:
        ob.matrix_world = m @ ob.matrix_world


def group_rotate_all(rot):
    rotate_objects([ob for ob in bpy.context.scene.collection.objects if ob.type == "MESH"], rot)


def sword(m_blade, m_hilt, tilt=18.0):
    # Composed vertically around z=0.3, then tilted as one group in the
    # camera plane so all parts stay attached.
    add(bpy.ops.mesh.primitive_cube_add, m_blade, loc=(0, 0, 0.55), scale=(0.14, 0.05, 0.95))
    add(bpy.ops.mesh.primitive_cone_add, m_blade, loc=(0, 0, 1.62), scale=(0.14, 0.05, 0.22))
    add(bpy.ops.mesh.primitive_cube_add, m_hilt, loc=(0, 0, -0.48), scale=(0.42, 0.08, 0.08))
    add(bpy.ops.mesh.primitive_cylinder_add, m_hilt, loc=(0, 0, -0.82), scale=(0.07, 0.07, 0.28))
    add(bpy.ops.mesh.primitive_uv_sphere_add, m_hilt, loc=(0, 0, -1.14), scale=(0.12, 0.12, 0.12))
    group_rotate_all((0, tilt, 0))


def arrow(m_shaft, m_head, loc=(0, 0, 0.3), tilt=30.0):
    # Vertical arrow parts offset by loc, tilted in the camera (x-z) plane.
    add(bpy.ops.mesh.primitive_cylinder_add, m_shaft, loc=(loc[0], loc[1], loc[2]), scale=(0.05, 0.05, 1.05))
    add(bpy.ops.mesh.primitive_cone_add, m_head, loc=(loc[0], loc[1], loc[2] + 1.2), scale=(0.15, 0.15, 0.28))
    add(bpy.ops.mesh.primitive_cube_add, m_head, loc=(loc[0] - 0.09, loc[1], loc[2] - 0.95), rot=(0, 35, 0), scale=(0.03, 0.02, 0.22))
    add(bpy.ops.mesh.primitive_cube_add, m_head, loc=(loc[0] + 0.09, loc[1], loc[2] - 0.95), rot=(0, -35, 0), scale=(0.03, 0.02, 0.22))
    group_rotate_all((0, tilt, 0))


def orb(m, loc=(0, 0, 0.4), r=0.8):
    bpy.ops.mesh.primitive_uv_sphere_add(location=loc, radius=r, segments=48, ring_count=24)
    ob = bpy.context.active_object
    bpy.ops.object.shade_smooth()
    ob.data.materials.append(m)
    return ob


def swirl(m, n=8, r=1.15, z=0.3):
    # Radial dashes in the camera (x-z) plane so the ring reads face-on.
    for i in range(n):
        a = 2 * math.pi * i / n
        add(bpy.ops.mesh.primitive_cube_add, m,
            loc=(r * math.cos(a), 0.3, z + r * math.sin(a)),
            rot=(0, -math.degrees(a), 0), scale=(0.30, 0.06, 0.06))


# ─── Icon scene builders ─────────────────────────────────────────────────────

def icon_attack():
    sword(STEEL(), WOOD(), tilt=18)

def icon_heroic_strike():
    sword(STEEL(), GOLD(), tilt=-24)
    orb(BLOOD(), loc=(0.85, -0.3, 1.15), r=0.26)

def icon_whirlwind():
    sword(STEEL(), WOOD(), tilt=55)
    swirl(mat("wind", (0.75, 0.85, 0.9), emit=(0.7, 0.85, 1.0), emit_str=1.2))

def icon_aimed_shot():
    m = mat("ring", (0.9, 0.2, 0.15), emit=(1.0, 0.2, 0.1), emit_str=1.5)
    bpy.ops.mesh.primitive_torus_add(location=(0, 0.5, 0.3), rotation=(math.radians(90), 0, 0), major_radius=0.95, minor_radius=0.05)
    bpy.context.active_object.data.materials.append(m)
    bpy.ops.mesh.primitive_torus_add(location=(0, 0.5, 0.3), rotation=(math.radians(90), 0, 0), major_radius=0.45, minor_radius=0.05)
    bpy.context.active_object.data.materials.append(m)
    arrow(WOOD(), STEEL(), tilt=32)

def icon_multi_shot():
    # Fan of three arrows; each is rotated only over the objects it added
    # (group_rotate_all would drag earlier arrows along with it).
    for ang in (-22, 0, 22):
        before = {ob.name for ob in bpy.context.scene.collection.objects}
        arrow(WOOD(), STEEL(), loc=(0, 0, 0.3), tilt=0)
        fresh = [ob for ob in bpy.context.scene.collection.objects
                 if ob.type == "MESH" and ob.name not in before]
        rotate_objects(fresh, (0, ang, 0), pivot=(0, 0, -0.8))

def icon_smite():
    m = HOLY()
    add(bpy.ops.mesh.primitive_cube_add, m, loc=(0, 0, 0.35), scale=(0.16, 0.16, 1.05))
    add(bpy.ops.mesh.primitive_cube_add, m, loc=(0, 0, 0.75), scale=(0.58, 0.16, 0.16))
    orb(mat("glow", (0.9, 0.75, 0.3), emit=(1, 0.8, 0.3), emit_str=0.5), loc=(0, 0.9, 0.35), r=1.15)

def icon_heal():
    m = mat("healcross", (0.15, 0.7, 0.3), emit=(0.15, 0.8, 0.25), emit_str=1.8)
    add(bpy.ops.mesh.primitive_cube_add, m, loc=(0, 0, 0.35), scale=(0.22, 0.22, 0.8))
    add(bpy.ops.mesh.primitive_cube_add, m, loc=(0, 0, 0.35), scale=(0.8, 0.22, 0.22))

def icon_firebolt():
    orb(FIRE(), loc=(-0.35, 0, 0.15), r=0.7)
    m = mat("trail", (1.0, 0.45, 0.05), emit=(1.0, 0.4, 0.03), emit_str=1.6)
    for i, s in enumerate((0.42, 0.28, 0.17)):
        orb(m, loc=(0.55 + i * 0.5, 0.15, 0.75 + i * 0.42), r=s)

def icon_frost_nova():
    orb(FROST(), loc=(0, 0, 0.3), r=0.5)
    m = FROST()
    for i in range(8):
        a = 2 * math.pi * i / 8
        add(bpy.ops.mesh.primitive_cone_add, m,
            loc=(1.0 * math.cos(a), 0, 0.3 + 1.0 * math.sin(a)),
            rot=(0, 90 - math.degrees(a), 0), scale=(0.13, 0.13, 0.45))


ICONS = {
    "attack": icon_attack,
    "heroic_strike": icon_heroic_strike,
    "whirlwind": icon_whirlwind,
    "aimed_shot": icon_aimed_shot,
    "multi_shot": icon_multi_shot,
    "smite": icon_smite,
    "heal": icon_heal,
    "firebolt": icon_firebolt,
    "frost_nova": icon_frost_nova,
}


def main():
    os.makedirs(OUT_DIR, exist_ok=True)
    for name, build in ICONS.items():
        reset_scene()
        build()
        path = os.path.join(OUT_DIR, f"{name}.png")
        bpy.context.scene.render.filepath = path
        bpy.ops.render.render(write_still=True)
        print(f"ICON OK {path}")
    print(f"DONE {len(ICONS)} icons -> {OUT_DIR}")


main()
