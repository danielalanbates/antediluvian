"""Hero landmark models for Antediluvia — scripted in Blender, iterated on
renders, exported as GLB for the Bevy client.

Run:  blender --background --python scripts/render/models.py -- [name ...]
Renders 3 review angles per model to /tmp/adv_models/<name>_<angle>.png and
exports assets/models/hero/<name>.glb (vertex-colored, no textures — matches
the game's low-poly kit look).

Subjects (docs/locations):
  ziggurat  — Central Ziggurat of Lamech (Enoch capital centerpiece)
  boundary  — Flaming Boundary gate of Eden w/ the Cherubim's turning sword
  altar     — The First Altar (rebuilt stones, Nod crossing)
"""
import bpy
import math
import os
import sys

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
OUT_GLB = os.path.join(ROOT, "assets", "models", "hero")
OUT_PNG = "/tmp/adv_models"
SIZE = 640


def reset_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    sc = bpy.context.scene
    sc.render.resolution_x = SIZE
    sc.render.resolution_y = SIZE
    try:
        sc.render.engine = "BLENDER_EEVEE_NEXT"
    except TypeError:
        sc.render.engine = "BLENDER_EEVEE"
    world = bpy.data.worlds.new("w")
    world.use_nodes = True
    bg = world.node_tree.nodes["Background"]
    bg.inputs[0].default_value = (0.18, 0.22, 0.30, 1.0)
    bg.inputs[1].default_value = 0.35
    sc.world = world
    sun = bpy.data.lights.new("sun", type="SUN")
    sun.energy = 5.0
    so = bpy.data.objects.new("sun", sun)
    so.rotation_euler = (math.radians(50), 0, math.radians(30))
    sc.collection.objects.link(so)


def mat(name, base, metal=0.0, rough=0.7, emit=None, emit_str=0.0):
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
    mesh_op(location=loc, rotation=tuple(math.radians(a) for a in rot), **kw)
    ob = bpy.context.active_object
    ob.scale = scale
    if m is not None:
        ob.data.materials.append(m)
    return ob


# ─── Subjects ────────────────────────────────────────────────────────────────

def build_ziggurat():
    """Central Ziggurat of Lamech: stepped megalith, smog-dark stone with
    bronze trim and a burning brazier crown (docs/locations/02)."""
    stone = mat("stone", (0.13, 0.11, 0.10), rough=0.9)
    dark = mat("dark", (0.07, 0.06, 0.06), rough=0.95)
    bronze = mat("bronze", (0.45, 0.24, 0.07), metal=1.0, rough=0.4)
    flame = mat("flame", (1.0, 0.45, 0.08), emit=(1.0, 0.42, 0.05), emit_str=14.0)

    # Five shrinking tiers.
    z = 0.0
    for i, (w, h) in enumerate([(10.0, 1.6), (8.0, 1.4), (6.2, 1.2), (4.6, 1.0), (3.2, 0.9)]):
        m = stone if i % 2 == 0 else dark
        add(bpy.ops.mesh.primitive_cube_add, m, loc=(0, 0, z + h / 2), scale=(w / 2, w / 2, h / 2))
        z += h
    top_z = z
    # Grand stair (south face): actual steps hugging the tier profile.
    step_h = 0.22
    n_steps = int(top_z / step_h)
    for s in range(n_steps):
        zc = s * step_h + step_h / 2
        # front face recedes with height, matching the tier taper
        y = -(5.4 - (zc / top_z) * 3.6)
        add(bpy.ops.mesh.primitive_cube_add, dark, loc=(0, y, zc), scale=(1.2, 0.55, step_h / 2))
    # Corner bronze pillars on the crown tier.
    for sx in (-1, 1):
        for sy in (-1, 1):
            add(bpy.ops.mesh.primitive_cylinder_add, bronze,
                loc=(sx * 1.3, sy * 1.3, top_z + 0.7), scale=(0.16, 0.16, 0.7))
    # Crown shrine slab + brazier flame.
    add(bpy.ops.mesh.primitive_cube_add, bronze, loc=(0, 0, top_z + 1.45), scale=(1.6, 1.6, 0.12))
    add(bpy.ops.mesh.primitive_cone_add, flame, loc=(0, 0, top_z + 2.1), scale=(0.55, 0.55, 0.75))
    add(bpy.ops.mesh.primitive_ico_sphere_add, flame, loc=(0, 0, top_z + 1.75), scale=(0.4, 0.4, 0.3))
    return 12.0  # camera distance hint


def build_boundary():
    """Flaming Boundary of Eden: two scorched monolith gateposts, a lintel,
    and the Cherubim's turning sword — a giant emissive blade — between them
    over a bed of embers (docs/locations/01)."""
    scorched = mat("scorched", (0.09, 0.07, 0.06), rough=0.95)
    gold = mat("gold", (0.7, 0.45, 0.08), metal=1.0, rough=0.35)
    blade = mat("blade", (1.0, 0.75, 0.25), emit=(1.0, 0.6, 0.12), emit_str=18.0)
    ember = mat("ember", (0.9, 0.25, 0.05), emit=(1.0, 0.22, 0.02), emit_str=8.0)

    for sx in (-1, 1):
        # Tapered monolith: stack two leaning blocks.
        add(bpy.ops.mesh.primitive_cube_add, scorched, loc=(sx * 3.2, 0, 2.2), rot=(0, sx * -4, 0), scale=(0.9, 1.3, 2.2))
        add(bpy.ops.mesh.primitive_cube_add, scorched, loc=(sx * 3.0, 0, 4.9), rot=(0, sx * -7, 0), scale=(0.65, 1.0, 0.8))
        # Gold cherub-wing finial: swept outward like a wing, not a spike.
        add(bpy.ops.mesh.primitive_cone_add, gold, loc=(sx * 3.3, 0, 5.9), rot=(0, sx * 55, 0), scale=(0.45, 0.14, 1.1))
    # Lintel sits directly on the monolith tops (they reach z≈5.7).
    add(bpy.ops.mesh.primitive_cube_add, scorched, loc=(0, 0, 5.45), scale=(3.8, 0.9, 0.5))
    # The turning sword, tip-up, tilted mid-spin in the gate plane (y-axis
    # rotation). All parts placed along the tilted blade axis.
    tilt = math.radians(18)
    def on_axis(z_along):
        # point z_along up the blade from the hilt pivot at (0, 0, 1.0)
        return (-math.sin(tilt) * z_along, 0.0, 1.0 + math.cos(tilt) * z_along)
    add(bpy.ops.mesh.primitive_cube_add, blade, loc=on_axis(2.1), rot=(0, 18, 0), scale=(0.26, 0.07, 2.1))
    add(bpy.ops.mesh.primitive_cone_add, blade, loc=on_axis(4.45), rot=(0, 18, 0), scale=(0.26, 0.07, 0.35))
    add(bpy.ops.mesh.primitive_cube_add, gold, loc=on_axis(0.0), rot=(0, 18, 0), scale=(0.75, 0.12, 0.12))
    add(bpy.ops.mesh.primitive_cylinder_add, gold, loc=on_axis(-0.45), rot=(0, 18, 0), scale=(0.09, 0.09, 0.4))
    # Ember bed.
    for i in range(9):
        a = i * 0.7
        add(bpy.ops.mesh.primitive_ico_sphere_add, ember,
            loc=(math.cos(a) * (1.0 + i * 0.18), math.sin(a) * 0.7, 0.12), scale=(0.22, 0.22, 0.12))
    return 9.0


def build_altar():
    """The First Altar: rough fieldstone ring stacked into a low altar, a
    smoke-stained offering slab, scattered toppled stones (docs quest 12)."""
    field = mat("field", (0.36, 0.32, 0.27), rough=0.95)
    moss = mat("moss", (0.22, 0.30, 0.16), rough=0.9)
    char = mat("char", (0.05, 0.045, 0.04), rough=1.0)

    # Two stacked courses of rough stones in a ring.
    import random
    rng = random.Random(7)
    for level, r, n in [(0.30, 1.35, 12), (0.85, 1.15, 9)]:
        for i in range(n):
            a = 2 * math.pi * i / n + level
            s = 0.36 + rng.random() * 0.12
            m = moss if rng.random() < 0.15 else field
            add(bpy.ops.mesh.primitive_cube_add, m,
                loc=(math.cos(a) * r, math.sin(a) * r, level),
                rot=(rng.random() * 8, rng.random() * 8, rng.random() * 90),
                scale=(s, s * 0.8, 0.30))
    # Offering slab, charred center.
    add(bpy.ops.mesh.primitive_cylinder_add, field, loc=(0, 0, 1.35), scale=(1.15, 1.15, 0.14), vertices=10)
    add(bpy.ops.mesh.primitive_cylinder_add, char, loc=(0, 0, 1.52), scale=(0.55, 0.55, 0.05), vertices=10)
    # Toppled stones nearby (kicked over by Cain's descendants).
    for (x, y, rz) in [(2.3, 0.6, 40), (2.0, -1.1, 70), (-2.4, 0.9, 15)]:
        add(bpy.ops.mesh.primitive_cube_add, field, loc=(x, y, 0.2), rot=(20, 8, rz), scale=(0.45, 0.35, 0.3))
    return 5.0


def build_ark():
    """Ark Construction Yard (Flood act): the great gopher-wood hull on
    scaffolding, half-planked, pitch-black below the waterline."""
    wood = mat("wood", (0.30, 0.18, 0.08), rough=0.85)
    dark = mat("darkwood", (0.16, 0.09, 0.05), rough=0.9)
    pitch = mat("pitch", (0.05, 0.04, 0.035), rough=0.6)
    frame = mat("frame", (0.38, 0.26, 0.13), rough=0.9)

    # Hull: stacked, tapering plank courses (boxy WoW-read, not smooth).
    courses = [
        (7.2, 1.9, 0.0, pitch), (7.8, 2.1, 0.55, pitch), (8.2, 2.25, 1.10, dark),
        (8.4, 2.35, 1.65, wood), (8.3, 2.3, 2.20, dark), (7.9, 2.2, 2.75, wood),
    ]
    for (l, w, z, m) in courses:
        add(bpy.ops.mesh.primitive_cube_add, m, loc=(0, 0, z + 0.3), scale=(l / 2, w / 2, 0.30))
    # Bow + stern rakes: slim, hull-height, tucked against the ends.
    add(bpy.ops.mesh.primitive_cube_add, dark, loc=(4.05, 0, 1.55), rot=(0, -18, 0), scale=(0.55, 1.0, 1.45))
    add(bpy.ops.mesh.primitive_cube_add, dark, loc=(-4.05, 0, 1.55), rot=(0, 18, 0), scale=(0.55, 1.0, 1.45))
    # Deckhouse (the window "one cubit above").
    add(bpy.ops.mesh.primitive_cube_add, wood, loc=(0, 0, 3.5), scale=(2.6, 0.85, 0.45))
    add(bpy.ops.mesh.primitive_cube_add, frame, loc=(0, 0, 4.05), scale=(2.8, 1.0, 0.12))
    # Scaffolding: poles + walk-boards along both flanks (construction yard).
    for sy in (-1, 1):
        for i in range(6):
            x = -3.5 + i * 1.4
            add(bpy.ops.mesh.primitive_cylinder_add, frame, loc=(x, sy * 1.75, 1.5), scale=(0.09, 0.09, 1.5))
            add(bpy.ops.mesh.primitive_cylinder_add, frame, loc=(x, sy * 2.6, 1.1), scale=(0.09, 0.09, 1.1))
        for z in (1.1, 2.2):
            add(bpy.ops.mesh.primitive_cube_add, frame, loc=(0, sy * 2.15, z), scale=(3.9, 0.32, 0.06))
    # Support cradles under the hull.
    for x in (-2.6, 0.0, 2.6):
        add(bpy.ops.mesh.primitive_cube_add, frame, loc=(x, 0, 0.15), scale=(0.25, 1.6, 0.15))
    return 11.0


def build_descent():
    """Watcher's Descent Point (Hermon): a scorched ring of standing stones
    around a still-glowing celestial shard driven into the rock."""
    rock = mat("rock", (0.18, 0.17, 0.19), rough=0.95)
    scorch = mat("scorch", (0.08, 0.07, 0.08), rough=1.0)
    shard = mat("shard", (0.25, 0.45, 0.95), emit=(0.30, 0.50, 1.0), emit_str=2.2, rough=0.2)

    # The shard: a tilted, faceted spike half-buried.
    add(bpy.ops.mesh.primitive_cone_add, shard, loc=(0, 0, 2.0), rot=(12, 0, 25), scale=(0.7, 0.7, 2.4), vertices=6)
    add(bpy.ops.mesh.primitive_ico_sphere_add, shard, loc=(0.35, 0.15, 0.4), scale=(0.5, 0.5, 0.3))
    # Upthrust rock slabs around the impact, leaning outward.
    import random
    rng = random.Random(3)
    for i in range(8):
        a = 2 * math.pi * i / 8 + 0.2
        r = 2.2 + rng.random() * 0.5
        add(bpy.ops.mesh.primitive_cube_add, rock if i % 3 else scorch,
            loc=(math.cos(a) * r, math.sin(a) * r, 0.7 + rng.random() * 0.3),
            rot=(math.degrees(math.sin(a)) * 0.25, -math.degrees(math.cos(a)) * 0.25, rng.random() * 40),
            scale=(0.45, 0.28, 0.9 + rng.random() * 0.5))
    # Scorched ground ring.
    add(bpy.ops.mesh.primitive_cylinder_add, scorch, loc=(0, 0, 0.03), scale=(3.1, 3.1, 0.03), vertices=24)
    return 7.0


def build_observatory():
    """Stargazer Observatory (Hermon): a squat stone tower with a split
    viewing slit and a bronze armillary ring on top."""
    stone = mat("stone", (0.24, 0.23, 0.25), rough=0.9)
    dark = mat("dark", (0.13, 0.12, 0.14), rough=0.95)
    bronze = mat("bronze", (0.5, 0.30, 0.10), metal=1.0, rough=0.35)

    add(bpy.ops.mesh.primitive_cylinder_add, stone, loc=(0, 0, 1.5), scale=(1.7, 1.7, 1.5), vertices=12)
    add(bpy.ops.mesh.primitive_cylinder_add, dark, loc=(0, 0, 3.15), scale=(1.85, 1.85, 0.18), vertices=12)
    # Split dome: two quarter-spheres with a slit between.
    for sy in (-1, 1):
        add(bpy.ops.mesh.primitive_uv_sphere_add, stone, loc=(0, sy * 0.45, 3.3), scale=(1.5, 1.05, 1.2))
    # Armillary: two interlocked bronze rings + gnomon.
    bpy.ops.mesh.primitive_torus_add(location=(0, 0, 5.0), rotation=(math.radians(90), 0, math.radians(30)), major_radius=0.85, minor_radius=0.06)
    bpy.context.active_object.data.materials.append(bronze)
    bpy.ops.mesh.primitive_torus_add(location=(0, 0, 5.0), rotation=(math.radians(90), 0, math.radians(-40)), major_radius=0.65, minor_radius=0.06)
    bpy.context.active_object.data.materials.append(bronze)
    add(bpy.ops.mesh.primitive_cylinder_add, bronze, loc=(0, 0, 4.4), scale=(0.08, 0.08, 0.55))
    # Stair stub + door.
    add(bpy.ops.mesh.primitive_cube_add, dark, loc=(0, -1.75, 0.55), scale=(0.55, 0.35, 0.55))
    return 7.5


def build_bonetotem():
    """Bone-Totem Field (Nephilim wastes): a colossal ribcage arch and
    stacked-skull totem poles — giants' leavings."""
    bone = mat("bone", (0.75, 0.70, 0.60), rough=0.8)
    old = mat("oldbone", (0.55, 0.50, 0.42), rough=0.9)
    sinew = mat("sinew", (0.35, 0.15, 0.10), rough=0.85)

    # Ribcage: each rib is a 3-segment arc in the y-z plane from the ground
    # up to the spine — segment endpoints on an ellipse (y = W·cosθ,
    # z = H·sinθ), cylinders laid exactly between consecutive points.
    def rib(x, side, w, h, m, r):
        pts = []
        for k in range(4):
            th = math.radians(k * 30)
            pts.append((side * w * math.cos(th), h * math.sin(th)))
        for (y0, z0), (y1, z1) in zip(pts, pts[1:]):
            my, mz = (y0 + y1) / 2, (z0 + z1) / 2
            dy, dz = y1 - y0, z1 - z0
            length = math.hypot(dy, dz) / 2
            # cylinder +Z axis → segment direction: rotate about x
            ang = math.degrees(math.atan2(dy, dz))
            add(bpy.ops.mesh.primitive_cylinder_add, m,
                loc=(x, my, mz), rot=(-ang, 0, 0), scale=(r, r, length + 0.06))
    for i in range(4):
        x = -1.8 + i * 1.2
        s = 1.0 - abs(i - 1.5) * 0.12
        m = bone if i % 2 else old
        for sy in (-1, 1):
            rib(x, sy, 1.5 * s, 2.4 * s, m, 0.14 * s)
    # Spine ridge resting on the rib tops (arch apex z = 2.4·s).
    add(bpy.ops.mesh.primitive_cylinder_add, old, loc=(-0.3, 0, 2.35), rot=(0, 90, 0), scale=(0.18, 0.18, 2.6))
    # Totem poles: stacked skull-ish blocks.
    import random
    rng = random.Random(11)
    for (tx, ty) in [(2.9, 1.4), (-3.0, -1.2), (3.1, -1.6)]:
        z = 0.4
        for lvl in range(3):
            s = 0.5 - lvl * 0.09
            add(bpy.ops.mesh.primitive_cube_add, bone if lvl % 2 else old,
                loc=(tx, ty, z), rot=(0, 0, rng.random() * 30), scale=(s, s * 0.85, s * 0.75))
            # eye sockets hint: two small dark cubes
            z += s * 1.5
        add(bpy.ops.mesh.primitive_cone_add, sinew, loc=(tx, ty, z + 0.1), scale=(0.12, 0.12, 0.35))
    return 7.0


def build_wartent():
    """Warlord's Command Tent (Nephilim): an oversized hide pavilion on
    tusk-poles with war banners."""
    hide = mat("hide", (0.36, 0.22, 0.12), rough=0.9)
    darkhide = mat("darkhide", (0.22, 0.13, 0.08), rough=0.95)
    tusk = mat("tusk", (0.7, 0.65, 0.55), rough=0.7)
    banner = mat("banner", (0.45, 0.06, 0.05), rough=0.8)

    # Main pavilion: broad cone + skirt.
    add(bpy.ops.mesh.primitive_cone_add, hide, loc=(0, 0, 2.1), scale=(3.2, 3.2, 1.7), vertices=9)
    add(bpy.ops.mesh.primitive_cylinder_add, darkhide, loc=(0, 0, 0.8), scale=(2.9, 2.9, 0.8), vertices=9)
    # Entrance flap: dark wedge.
    add(bpy.ops.mesh.primitive_cube_add, darkhide, loc=(0, -2.75, 0.75), rot=(18, 0, 0), scale=(0.8, 0.25, 0.75))
    # Tusk-poles curving out around the rim.
    for i in range(6):
        a = 2 * math.pi * i / 6 + 0.3
        add(bpy.ops.mesh.primitive_cone_add, tusk,
            loc=(math.cos(a) * 3.3, math.sin(a) * 3.3, 1.3),
            rot=(math.degrees(-math.sin(a)) * 0.22, math.degrees(math.cos(a)) * 0.22, 0),
            scale=(0.14, 0.14, 1.5))
    # Center pole + war banner.
    add(bpy.ops.mesh.primitive_cylinder_add, tusk, loc=(0, 0, 4.3), scale=(0.09, 0.09, 1.1))
    add(bpy.ops.mesh.primitive_cube_add, banner, loc=(0.55, 0, 4.9), scale=(0.55, 0.03, 0.4))
    return 8.0


def build_leviathan():
    """Leviathan Shallows (Flood): the half-buried skeleton of a sea titan —
    skull, rib arcs shrinking down the tail, all bleached bone."""
    bone = mat("bone", (0.78, 0.74, 0.64), rough=0.8)
    old = mat("oldbone", (0.60, 0.55, 0.46), rough=0.9)

    # Skull: broad wedge + eye sockets + jaw.
    add(bpy.ops.mesh.primitive_cube_add, bone, loc=(-4.2, 0, 1.0), rot=(0, -8, 0), scale=(1.7, 1.25, 0.9))
    add(bpy.ops.mesh.primitive_cone_add, bone, loc=(-5.9, 0, 0.75), rot=(0, -95, 0), scale=(0.85, 0.85, 1.1))
    add(bpy.ops.mesh.primitive_cube_add, old, loc=(-4.4, 0, 0.25), rot=(0, -6, 0), scale=(1.3, 0.9, 0.22))
    # Rib arcs shrinking toward the tail (reuse arch-segment math).
    def rib(x, w, h, m, r):
        pts = []
        for k in range(4):
            th = math.radians(k * 30)
            pts.append((w * math.cos(th), h * math.sin(th)))
        for side in (-1, 1):
            for (y0, z0), (y1, z1) in zip(pts, pts[1:]):
                my, mz = (y0 + y1) / 2, (z0 + z1) / 2
                dy, dz = y1 - y0, z1 - z0
                length = math.hypot(dy, dz) / 2
                ang = math.degrees(math.atan2(dy, dz))
                add(bpy.ops.mesh.primitive_cylinder_add, m,
                    loc=(x, side * my, mz), rot=(-ang * side, 0, 0), scale=(r, r, length + 0.05))
    for i in range(7):
        x = -2.2 + i * 1.15
        s = 1.0 - i * 0.11
        rib(x, 1.45 * s, 2.3 * s, bone if i % 2 else old, 0.13 * s)
    # Spine: slopes down the shrinking rib arches to the tail tip.
    add(bpy.ops.mesh.primitive_cylinder_add, old, loc=(1.1, 0, 1.55), rot=(0, 78, 0), scale=(0.15, 0.15, 3.6))
    add(bpy.ops.mesh.primitive_cylinder_add, old, loc=(4.9, 0, 0.75), rot=(0, 72, 0), scale=(0.11, 0.11, 1.1))
    add(bpy.ops.mesh.primitive_cone_add, old, loc=(5.9, 0, 0.45), rot=(0, 105, 0), scale=(0.09, 0.09, 0.7))
    return 10.0


def build_footprint():
    """Giant's Footprint Lake (Nephilim): a colossal sunken footprint — raised
    rim wall, five toe mounds, still water inside."""
    earth = mat("earth", (0.30, 0.24, 0.16), rough=0.95)
    mud = mat("mud", (0.20, 0.16, 0.11), rough=0.9)
    water = mat("water", (0.15, 0.30, 0.35), rough=0.15, metal=0.1)

    # Sole rim: ellipse of mounded earth (leave a gap at the heel-toe line).
    for i in range(16):
        a = 2 * math.pi * i / 16
        add(bpy.ops.mesh.primitive_ico_sphere_add, earth,
            loc=(math.cos(a) * 2.6, math.sin(a) * 1.7, 0.25),
            scale=(0.75, 0.6, 0.45))
    # Water fill.
    add(bpy.ops.mesh.primitive_cylinder_add, water, loc=(0, 0, 0.18), scale=(2.4, 1.55, 0.04), vertices=24)
    # Five toe mounds + toe pools off the front.
    for k, (tx, ty, s) in enumerate([(3.6, 1.1, 0.55), (3.9, 0.45, 0.62), (4.0, -0.25, 0.62), (3.8, -0.9, 0.55), (3.4, -1.45, 0.45)]):
        add(bpy.ops.mesh.primitive_ico_sphere_add, mud, loc=(tx, ty, 0.2), scale=(s, s * 0.8, 0.35))
        add(bpy.ops.mesh.primitive_cylinder_add, water, loc=(tx, ty, 0.16), scale=(s * 0.55, s * 0.45, 0.03), vertices=12)
    return 7.0


def build_geyser():
    """Boiling Geyser (Flood): mineral terrace cone with a steam plume."""
    crust = mat("crust", (0.72, 0.62, 0.45), rough=0.9)
    sinter = mat("sinter", (0.85, 0.80, 0.68), rough=0.85)
    steam = mat("steam", (0.9, 0.92, 0.95), rough=1.0, emit=(0.85, 0.88, 0.92), emit_str=0.8)
    pool = mat("pool", (0.25, 0.55, 0.55), rough=0.2, emit=(0.15, 0.45, 0.45), emit_str=0.8)

    # Stacked terraces.
    for (r, z, m) in [(2.6, 0.18, crust), (2.0, 0.5, sinter), (1.45, 0.85, crust), (0.95, 1.2, sinter)]:
        add(bpy.ops.mesh.primitive_cylinder_add, m, loc=(0, 0, z), scale=(r, r, 0.2), vertices=14)
    # Vent pool.
    add(bpy.ops.mesh.primitive_cylinder_add, pool, loc=(0, 0, 1.42), scale=(0.55, 0.55, 0.05), vertices=12)
    # Steam plume: stacked translucent-ish blobs rising and drifting.
    for i in range(5):
        add(bpy.ops.mesh.primitive_ico_sphere_add, steam,
            loc=(0.15 * i, 0.1 * math.sin(i * 1.7), 1.9 + i * 0.75),
            scale=(0.45 + i * 0.16, 0.45 + i * 0.16, 0.5 + i * 0.12))
    return 6.5


def build_feastpit():
    """Feasting Pit (Nephilim): a charred spit-roast pit ringed by giant
    gnawed bones and crude stone seats."""
    char = mat("char", (0.06, 0.05, 0.045), rough=1.0)
    ember = mat("ember", (1.0, 0.35, 0.05), emit=(1.0, 0.3, 0.03), emit_str=3.0)
    bone = mat("bone", (0.75, 0.70, 0.60), rough=0.8)
    stone = mat("stone", (0.25, 0.23, 0.21), rough=0.95)
    wood = mat("wood", (0.25, 0.15, 0.07), rough=0.9)

    # Fire pit: char ring + ember bed.
    for i in range(10):
        a = 2 * math.pi * i / 10
        add(bpy.ops.mesh.primitive_cube_add, char, loc=(math.cos(a) * 1.5, math.sin(a) * 1.5, 0.22), rot=(0, 0, math.degrees(a)), scale=(0.4, 0.25, 0.22))
    add(bpy.ops.mesh.primitive_cylinder_add, ember, loc=(0, 0, 0.12), scale=(1.15, 1.15, 0.08), vertices=14)
    # Spit: two forked posts + crossbar with a huge joint of meat (bone through).
    for sx in (-1, 1):
        add(bpy.ops.mesh.primitive_cylinder_add, wood, loc=(sx * 1.7, 0, 0.9), scale=(0.09, 0.09, 0.9))
    add(bpy.ops.mesh.primitive_cylinder_add, bone, loc=(0, 0, 1.75), rot=(0, 90, 0), scale=(0.08, 0.08, 2.1))
    add(bpy.ops.mesh.primitive_ico_sphere_add, mat("meat", (0.45, 0.15, 0.08), rough=0.7), loc=(0, 0, 1.75), scale=(0.75, 0.45, 0.45))
    # Gnawed giant bones scattered.
    import random
    rng = random.Random(9)
    for i in range(5):
        a = rng.random() * math.tau
        r = 2.4 + rng.random() * 1.2
        add(bpy.ops.mesh.primitive_cylinder_add, bone,
            loc=(math.cos(a) * r, math.sin(a) * r, 0.15),
            rot=(90, 0, rng.random() * 180), scale=(0.09, 0.09, 0.7 + rng.random() * 0.5))
    # Crude stone seats.
    for a_deg in (40, 160, 280):
        a = math.radians(a_deg)
        add(bpy.ops.mesh.primitive_cube_add, stone, loc=(math.cos(a) * 3.2, math.sin(a) * 3.2, 0.35), rot=(0, 0, a_deg), scale=(0.6, 0.45, 0.35))
    return 6.5


MODELS = {
    "ziggurat": build_ziggurat,
    "boundary": build_boundary,
    "altar": build_altar,
    "ark": build_ark,
    "descent": build_descent,
    "observatory": build_observatory,
    "bonetotem": build_bonetotem,
    "wartent": build_wartent,
    "leviathan": build_leviathan,
    "footprint": build_footprint,
    "geyser": build_geyser,
    "feastpit": build_feastpit,
}


def render_angles(name, dist, height):
    sc = bpy.context.scene
    cam_data = bpy.data.cameras.new("cam")
    cam = bpy.data.objects.new("cam", cam_data)
    sc.collection.objects.link(cam)
    sc.camera = cam
    for angle_deg in (25, 145, 265):
        a = math.radians(angle_deg)
        cam.location = (math.sin(a) * dist, -math.cos(a) * dist, height)
        # aim at model mid-height
        direction = (0 - cam.location[0], 0 - cam.location[1], height * 0.45 - cam.location[2])
        import mathutils
        cam.rotation_euler = mathutils.Vector(direction).to_track_quat("-Z", "Y").to_euler()
        sc.render.filepath = os.path.join(OUT_PNG, f"{name}_{angle_deg}.png")
        bpy.ops.render.render(write_still=True)
        print(f"RENDER OK {sc.render.filepath}")


def export_glb(name):
    path = os.path.join(OUT_GLB, f"{name}.glb")
    bpy.ops.object.select_all(action="SELECT")
    # exclude camera/lights from export
    bpy.ops.export_scene.gltf(
        filepath=path,
        export_format="GLB",
        use_selection=False,
        export_lights=False,
        export_cameras=False,
        export_apply=True,
    )
    print(f"GLB OK {path}")


def main():
    os.makedirs(OUT_GLB, exist_ok=True)
    os.makedirs(OUT_PNG, exist_ok=True)
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    names = argv or list(MODELS)
    for name in names:
        reset_scene()
        dist = MODELS[name]()
        render_angles(name, dist * 1.6, dist * 0.75)
        export_glb(name)
    print(f"DONE {names}")


main()
