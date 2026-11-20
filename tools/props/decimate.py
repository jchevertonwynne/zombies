"""Run by Blender, headless, from `props.py`:

    blender --background --python tools/props/decimate.py -- <in.gltf> <out.gltf> <length> <height> <depth> <max_tris>

Imports a Poly Haven model, joins its meshes, decimates it to at most `max_tris` triangles, and
stands it as the game's fitting framework expects a unit: its foot's middle at the origin, its length
along +X and its depth along +Z (glTF's Y is up). `length`, `height` and `depth` are the unit's size
in metres; the model is scaled to it per axis. A 0 keeps that axis's modelled size, scaled like the
given ones (all 0 leaves the model's own size). Writes the glTF with its mesh and UVs only:
`props.py` gives it its materials, in KTX2. Prints `UNIT x y z tris=n`.
"""

import sys

import bpy
from mathutils import Vector

args = sys.argv[sys.argv.index("--") + 1:]
src, dst = args[0], args[1]
want = [float(a) for a in args[2:5]]  # length, height, depth
max_tris = int(args[5])

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=src)

meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
if not meshes:
    sys.exit("no mesh in " + src)
bpy.ops.object.select_all(action="SELECT")
bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
bpy.ops.object.select_all(action="DESELECT")
for o in meshes:
    o.select_set(True)
bpy.context.view_layer.objects.active = meshes[0]
bpy.ops.object.join()
obj = bpy.context.view_layer.objects.active

# One material slot: the game wears the whole unit in one material.
while len(obj.material_slots) > 1:
    obj.active_material_index = len(obj.material_slots) - 1
    bpy.ops.object.material_slot_remove()

bpy.ops.object.mode_set(mode="EDIT")
bpy.ops.mesh.select_all(action="SELECT")
bpy.ops.mesh.quads_convert_to_tris()
bpy.ops.object.mode_set(mode="OBJECT")

tris = len(obj.data.polygons)
if tris > max_tris:
    mod = obj.modifiers.new("decimate", "DECIMATE")
    mod.decimate_type = "COLLAPSE"
    mod.ratio = max_tris / tris * 0.97
    mod.use_collapse_triangulate = True
    bpy.ops.object.modifier_apply(modifier=mod.name)


def box():
    pts = [obj.matrix_world @ Vector(c) for c in obj.bound_box]
    return Vector(min(p[i] for p in pts) for i in range(3)), Vector(max(p[i] for p in pts) for i in range(3))


# Blender is Z-up: x is length, y depth, z height. Turn a quarter if the model is longer along Y.
lo, hi = box()
if hi.y - lo.y > hi.x - lo.x:
    obj.rotation_euler.z += 1.5707963
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    lo, hi = box()
size = hi - lo
unit = [want[0], want[2], want[1]]
given = [unit[i] / size[i] for i in range(3) if unit[i] > 0]
uniform = given[0] if given else 1.0
obj.scale = Vector((unit[i] / size[i]) if unit[i] > 0 else uniform for i in range(3))
bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
lo, hi = box()
obj.location = Vector((-(lo.x + hi.x) / 2, -(lo.y + hi.y) / 2, -lo.z))
bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
lo, hi = box()
size = hi - lo

bpy.ops.export_scene.gltf(
    filepath=dst,
    export_format="GLTF_SEPARATE",
    export_image_format="NONE",
    export_materials="NONE",
    export_yup=True,
    export_cameras=False,
    export_lights=False,
)
print("UNIT %.3f %.3f %.3f tris=%d" % (size.x, size.z, size.y, len(obj.data.polygons)))
