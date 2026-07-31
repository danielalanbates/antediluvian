"""Title splash key art for Antediluvia — 1920x1080 Blender render.

Run:  blender --background --python scripts/render/splash.py
Writes assets/art/splash.png

Composition: the Flaming Boundary gate at dusk on a ridge, the Cherubim's
sword blazing, with the Ziggurat of Lamech dark on the horizon — the game's
two poles (Eden's mercy, Enoch's corruption) in one frame.
"""
import bpy
import math
import os

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
OUT = os.path.join(ROOT, "assets", "art")


def mat(name, base, metal=0.0, rough=0.7, emit=None, emit_str=0.0):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    b = m.node_tree.nodes["Principled BSDF"]
    b.inputs["Base Color"].default_value = (*base, 1.0)
    b.inputs["Metallic"].default_value = metal
    b.inputs["Roughness"].default_value = rough
    if emit is not None:
        b.inputs["Emission Color"].default_value = (*emit, 1.0)
        b.inputs["Emission Strength"].default_value = emit_str
    return m


def add(op, m, loc=(0, 0, 0), rot=(0, 0, 0), scale=(1, 1, 1), **kw):
    op(location=loc, rotation=tuple(math.radians(a) for a in rot), **kw)
    ob = bpy.context.active_object
    ob.scale = scale
    if m is not None:
        ob.data.materials.append(m)
    return ob


bpy.ops.wm.read_factory_settings(use_empty=True)
sc = bpy.context.scene
sc.render.resolution_x = 1920
sc.render.resolution_y = 1080
try:
    sc.render.engine = "BLENDER_EEVEE_NEXT"
except TypeError:
    sc.render.engine = "BLENDER_EEVEE"

# Dusk sky: deep indigo up top, ember-orange at the horizon via a gradient world.
world = bpy.data.worlds.new("w")
world.use_nodes = True
nt = world.node_tree
bg = nt.nodes["Background"]
grad = nt.nodes.new("ShaderNodeTexGradient")
map_ = nt.nodes.new("ShaderNodeMapping")
coord = nt.nodes.new("ShaderNodeTexCoord")
ramp = nt.nodes.new("ShaderNodeValToRGB")
ramp.color_ramp.elements[0].position = 0.28
ramp.color_ramp.elements[0].color = (0.38, 0.11, 0.03, 1)   # horizon ember
ramp.color_ramp.elements[1].position = 0.40
ramp.color_ramp.elements[1].color = (0.02, 0.025, 0.07, 1)  # night indigo
map_.inputs["Rotation"].default_value = (0, math.radians(-90), 0)
nt.links.new(coord.outputs["Window"], map_.inputs["Vector"])
nt.links.new(map_.outputs["Vector"], grad.inputs["Vector"])
nt.links.new(grad.outputs["Fac"], ramp.inputs["Fac"])
nt.links.new(ramp.outputs["Color"], bg.inputs["Color"])
bg.inputs[1].default_value = 0.45
sc.world = world

# Low warm sun from behind the gate + cool fill.
sun = bpy.data.lights.new("sun", type="SUN")
sun.energy = 2.0
sun.color = (1.0, 0.55, 0.3)
so = bpy.data.objects.new("sun", sun)
so.rotation_euler = (math.radians(82), 0, math.radians(195))
sc.collection.objects.link(so)
fill = bpy.data.lights.new("fill", type="SUN")
fill.energy = 0.5
fill.color = (0.4, 0.5, 0.9)
fo = bpy.data.objects.new("fill", fill)
fo.rotation_euler = (math.radians(45), 0, math.radians(40))
sc.collection.objects.link(fo)

# ── Ground ridge ──
ground = mat("ground", (0.10, 0.09, 0.08), rough=1.0)
add(bpy.ops.mesh.primitive_plane_add, ground, loc=(0, 0, 0), scale=(60, 60, 1))
# Rolling foreground rocks.
import random
rng = random.Random(5)
rock = mat("rock", (0.08, 0.07, 0.07), rough=0.95)
for i in range(14):
    a = rng.random() * math.tau
    r = 6 + rng.random() * 18
    add(bpy.ops.mesh.primitive_ico_sphere_add, rock,
        loc=(math.cos(a) * r, math.sin(a) * r - 4, rng.random() * 0.3),
        scale=(0.8 + rng.random() * 1.6, 0.8 + rng.random() * 1.2, 0.5 + rng.random() * 0.8))

# ── The Flaming Boundary gate (center-frame) ──
scorched = mat("scorched", (0.07, 0.055, 0.05), rough=0.95)
gold = mat("gold", (0.7, 0.45, 0.08), metal=1.0, rough=0.35)
blade = mat("blade", (1.0, 0.75, 0.25), emit=(1.0, 0.55, 0.10), emit_str=25.0)
ember = mat("ember", (0.9, 0.25, 0.05), emit=(0.9, 0.15, 0.01), emit_str=3.5)

for sx in (-1, 1):
    add(bpy.ops.mesh.primitive_cube_add, scorched, loc=(sx * 3.2, 0, 2.2), rot=(0, sx * -4, 0), scale=(0.9, 1.3, 2.2))
    add(bpy.ops.mesh.primitive_cube_add, scorched, loc=(sx * 3.0, 0, 4.9), rot=(0, sx * -7, 0), scale=(0.65, 1.0, 0.8))
    add(bpy.ops.mesh.primitive_cone_add, gold, loc=(sx * 3.3, 0, 5.9), rot=(0, sx * 55, 0), scale=(0.45, 0.14, 1.1))
add(bpy.ops.mesh.primitive_cube_add, scorched, loc=(0, 0, 5.45), scale=(3.8, 0.9, 0.5))
tilt = math.radians(18)
def on_axis(z):
    return (-math.sin(tilt) * z, 0.0, 1.0 + math.cos(tilt) * z)
add(bpy.ops.mesh.primitive_cube_add, blade, loc=on_axis(1.9), rot=(0, 18, 0), scale=(0.24, 0.07, 1.9))
add(bpy.ops.mesh.primitive_cube_add, gold, loc=on_axis(0.0), rot=(0, 18, 0), scale=(0.75, 0.12, 0.12))
for i in range(12):
    a = i * 0.62
    add(bpy.ops.mesh.primitive_ico_sphere_add, ember,
        loc=(math.cos(a) * (1.0 + i * 0.16), math.sin(a) * 0.8, 0.10), scale=(0.14, 0.14, 0.07))
# A point light inside the blade so the gate glows onto the stone.
gl = bpy.data.lights.new("glow", type="POINT")
gl.energy = 3000
gl.color = (1.0, 0.5, 0.12)
go = bpy.data.objects.new("glow", gl)
go.location = (0, -0.5, 3.0)
sc.collection.objects.link(go)

# ── Ziggurat silhouette on the horizon (right third) ──
sil = mat("sil", (0.03, 0.028, 0.032), rough=1.0)
z = 0.0
for (w, h) in [(10, 1.6), (8, 1.4), (6.2, 1.2), (4.6, 1.0), (3.2, 0.9)]:
    add(bpy.ops.mesh.primitive_cube_add, sil, loc=(26, 34, z + h / 2), scale=(w / 2, w / 2, h / 2))
    z += h
flame2 = mat("flame2", (1.0, 0.45, 0.08), emit=(1.0, 0.42, 0.05), emit_str=20.0)
add(bpy.ops.mesh.primitive_cone_add, flame2, loc=(26, 34, z + 0.8), scale=(0.7, 0.7, 0.9))

# ── Camera: low hero angle looking through the gate toward the horizon ──
cam_data = bpy.data.cameras.new("cam")
cam_data.lens = 32
cam = bpy.data.objects.new("cam", cam_data)
sc.collection.objects.link(cam)
sc.camera = cam
cam.location = (-1.5, -14.0, 2.2)
import mathutils
direction = mathutils.Vector((1.5, 20.0, 2.2)) - mathutils.Vector(cam.location)
cam.rotation_euler = direction.to_track_quat("-Z", "Y").to_euler()

os.makedirs(OUT, exist_ok=True)
sc.render.filepath = os.path.join(OUT, "splash.png")
bpy.ops.render.render(write_still=True)
print(f"SPLASH OK {sc.render.filepath}")
