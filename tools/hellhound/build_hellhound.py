#!/usr/bin/env python3
"""Build the Hellhound as rigid parts in headless Blender and export it as glTF.

    blender --background --python tools/hellhound/build_hellhound.py -- [--out <file.gltf>]

Normally run through `tools/hellhound/build.py`, which then puts the CC0 hide's KTX2 textures on it.
Writes `assets/models/hellhound/hellhound.gltf` (and its `.bin`). The game loads it as scene 0 and
finds the parts by node name (see `src/enemies/hellhound_rig.rs`): Torso, Head, Jaw, Tail, LegFL,
LegFR, LegBL, LegBR, and EyeL, EyeR (drawn emissive by the game whatever the file says). Each
part's origin sits on its joint (hip, neck base, jaw hinge, tail root), so rotating the node about
its own origin poses it.

The model is sized to the box hound it replaces, centred on the same point: the body 0.4 m across
and 1.0 m long with its head on, the legs reaching the floor 0.37 m below the centre. It faces
Blender +Y, which glTF's -Z (the game's forward) is made from. The hit areas are never taken from
the mesh. Every part is box-projected into UVs at one metre a tile.

Triangle budget: well under 2k.
"""

import math
import os
import sys

import bpy
import bmesh
from mathutils import Vector

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))


def args():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    out = os.path.join(ROOT, "assets", "models", "hellhound", "hellhound.gltf")
    it = iter(argv)
    for arg in it:
        if arg == "--out":
            out = next(it)
    return out


def clear():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def hide_material():
    """A flat dark hide. `tools/hellhound/build.py` swaps in the KTX2 hide textures afterwards."""
    material = bpy.data.materials.new("Hide")
    material.use_nodes = True
    bsdf = material.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (0.12, 0.08, 0.07, 1.0)
    bsdf.inputs["Roughness"].default_value = 0.85
    return material


def eye_material():
    material = bpy.data.materials.new("Eye")
    material.use_nodes = True
    bsdf = material.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (1.0, 0.3, 0.05, 1.0)
    bsdf.inputs["Emission Color"].default_value = (1.0, 0.25, 0.03, 1.0)
    bsdf.inputs["Emission Strength"].default_value = 10.0
    return material


def box_uv(obj, tile):
    """World-scale box-projected UVs: each face projects along its dominant axis."""
    mesh = obj.data
    bm = bmesh.new()
    bm.from_mesh(mesh)
    layer = bm.loops.layers.uv.verify()
    for face in bm.faces:
        n = face.normal
        axis = max(range(3), key=lambda i: abs(n[i]))
        for loop in face.loops:
            p = loop.vert.co
            u, v = [(p.y, p.z), (p.x, p.z), (p.x, p.y)][axis]
            loop[layer].uv = (u / tile, v / tile)
    bm.to_mesh(mesh)
    bm.free()


def part(name, kind, size, at, material, origin=None, rotation=(0, 0, 0)):
    """A rigid part: `kind` ('box' or 'sphere') of `size` (x, y, z) centred at `at`, whose object
    origin is moved to `origin` (its joint) when given."""
    if kind == "sphere":
        bpy.ops.mesh.primitive_uv_sphere_add(segments=12, ring_count=8, radius=0.5, location=at)
    else:
        bpy.ops.mesh.primitive_cube_add(size=1.0, location=at)
    obj = bpy.context.active_object
    obj.name = name
    obj.scale = size
    obj.rotation_euler = rotation
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)
    if origin is not None:
        bpy.context.scene.cursor.location = origin
        bpy.ops.object.origin_set(type="ORIGIN_CURSOR")
    obj.data.materials.append(material)
    box_uv(obj, tile=1.0)
    return obj


def build():
    hide = hide_material()
    eyes = eye_material()
    # Blender +Y is the hound's forward; glTF export turns it into -Z. Floor is z = -0.37.
    torso = part("Torso", "sphere", (0.40, 0.62, 0.40), (0, 0, 0.10), hide)
    # The neck base is the head's pivot; the jaw hinges under the head.
    neck = Vector((0, 0.40, 0.15))
    head = part("Head", "box", (0.26, 0.30, 0.24), (0, 0.62, 0.30), hide, origin=neck)
    jaw = part("Jaw", "box", (0.16, 0.22, 0.07), (0, 0.72, 0.17), hide, origin=Vector((0, 0.60, 0.20)))
    jaw.parent = head
    for name, x in (("EyeL", -0.06), ("EyeR", 0.06)):
        eye = part(name, "sphere", (0.06, 0.06, 0.06), (x, 0.74, 0.34), eyes)
        eye.parent = head
    tail = part("Tail", "box", (0.07, 0.40, 0.07), (0, -0.55, 0.20), hide, origin=Vector((0, -0.34, 0.15)),
                rotation=(math.radians(25), 0, 0))
    hips = {"LegFL": (-0.13, 0.30), "LegFR": (0.13, 0.30), "LegBL": (-0.13, -0.30), "LegBR": (0.13, -0.30)}
    for name, (x, y) in hips.items():
        # 0.35 m legs from the hip at z = -0.02 down to the floor.
        part(name, "box", (0.10, 0.10, 0.35), (x, y, -0.195), hide, origin=Vector((x, y, -0.02)))
    # Ears, so the silhouette reads as a hound's.
    for name, x in (("EarL", -0.09), ("EarR", 0.09)):
        ear = part(name, "box", (0.05, 0.05, 0.12), (x, 0.55, 0.47), hide)
        ear.parent = head
    return torso


def export(out):
    os.makedirs(os.path.dirname(out), exist_ok=True)
    bpy.ops.export_scene.gltf(
        filepath=out,
        export_format="GLTF_SEPARATE",
        export_yup=True,
        export_apply=True,
        export_materials="EXPORT",
        export_animations=False,
        export_cameras=False,
        export_lights=False,
    )
    print("wrote", out)


def main():
    out = args()
    clear()
    build()
    tris = sum(len(o.data.polygons) for o in bpy.data.objects if o.type == "MESH")
    print("faces:", tris)
    export(out)


if __name__ == "__main__":
    main()
