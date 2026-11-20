"""Headless Blender kitbashes for the Crate, Sandbags and Headstone Props.

Run by `build_props.py`, not by hand:

    blender -b --python tools/props/kitbash.py -- --src <dir> --out <dir> [--headstone <scan>]

Each unit is one mesh of at most `BUDGET` triangles, written as a glTF with no textures and no
material beyond a name; `build_props.py` then points it at the KTX2 files. A unit's origin is the
middle of its foot, its length runs along +X in the game and its depth along +Z (the Prop-fitting
rule in `src/maps/map/fitting.rs`). Blender is Z-up, so the length is X, the depth is Y and the
height is Z here, and the glTF exporter turns that into the game's axes.

Everything is deterministic: the same sources give the same bytes.
"""

import argparse
import json
import math
import pathlib
import random
import sys

import bmesh
import bpy
from mathutils import Matrix, Vector

# A unit's triangle limit, from the Spec: at most about 2k.
BUDGET = 2000

# A Crate's layer is the model's 0.464 m stretched to this, so a stack of two is 1.06 m and a stack
# of three is 1.59 m: the heights the Props want (0.9 to 1.2, and 1.4 to 1.6) within the +-15% fit.
CRATE_LAYER = 0.53
# A stack is this wide and deep: two crates (1.166 m long, 0.529 m wide) a course, crossed, with
# a small gap down the middle.
CRATE_SPAN = 1.15
CRATE_TRIS = 325

# Sandbags: a course is 0.21 m high, a bag across the wall is 0.74 m long (the cement bag is
# 0.70 m) and 0.5 m wide along it, and a unit is 1.0 m long and about 0.8 m deep.
BAG_COURSE = 0.21
BAG_LENGTH = 0.72
BAG_WIDTH = 0.50
BAG_TRIS = 165
# Burlap's real-world swatch (Poly Haven `hessian_230`), in metres: the UV tile.
BURLAP_TILE = 0.2688

HEADSTONE_SIZE = (0.8, 0.25, 0.9)  # Blender: x wide, y deep, z tall
HEADSTONE_TRIS = 1800


def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def select_only(objects):
    bpy.ops.object.select_all(action="DESELECT")
    for obj in objects:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = objects[0]


def import_meshes(path):
    """The mesh objects of a glTF, joined into one object with its transforms applied."""
    before = set(bpy.data.objects)
    ext = pathlib.Path(path).suffix.lower()
    if ext in (".gltf", ".glb"):
        bpy.ops.import_scene.gltf(filepath=str(path))
    elif ext == ".obj":
        bpy.ops.wm.obj_import(filepath=str(path))
    else:
        raise SystemExit(f"cannot import {path}")
    meshes = [o for o in bpy.data.objects if o not in before and o.type == "MESH"]
    for obj in bpy.data.objects:
        if obj not in before and obj.type != "MESH":
            bpy.data.objects.remove(obj)
    select_only(meshes)
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    if len(meshes) > 1:
        bpy.ops.object.join()
    return bpy.context.view_layer.objects.active


def triangles(mesh):
    mesh.calc_loop_triangles()
    return len(mesh.loop_triangles)


def decimate(obj, target):
    """Collapse `obj` to about `target` triangles."""
    select_only([obj])
    mod = obj.modifiers.new("decimate", "DECIMATE")
    mod.decimate_type = "COLLAPSE"
    mod.use_collapse_triangulate = True
    mod.ratio = min(1.0, target / triangles(obj.data))
    bpy.ops.object.modifier_apply(modifier=mod.name)
    return obj


def bounds(mesh):
    xs = [v.co for v in mesh.vertices]
    lo = Vector((min(v.x for v in xs), min(v.y for v in xs), min(v.z for v in xs)))
    hi = Vector((max(v.x for v in xs), max(v.y for v in xs), max(v.z for v in xs)))
    return lo, hi


def ground(mesh):
    """Moves the mesh so the middle of its foot is the origin."""
    lo, hi = bounds(mesh)
    mesh.transform(Matrix.Translation(Vector((-(lo.x + hi.x) / 2, -(lo.y + hi.y) / 2, -lo.z))))


def combine(parts, name):
    """One mesh object from several (mesh, matrix) placements of copies."""
    bm = bmesh.new()
    for mesh, matrix in parts:
        copy = mesh.copy()
        copy.transform(matrix)
        bm.from_mesh(copy)
        bpy.data.meshes.remove(copy)
    out = bpy.data.meshes.new(name)
    bm.to_mesh(out)
    bm.free()
    obj = bpy.data.objects.new(name, out)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def single_material(obj, name):
    mesh = obj.data
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    mesh.materials.clear()
    mesh.materials.append(mat)
    for poly in mesh.polygons:
        poly.material_index = 0
    for poly in mesh.polygons:
        poly.use_smooth = True


def export(obj, out, name):
    """Writes `out/name.gltf` and `name.bin`, with no images."""
    select_only([obj])
    bpy.ops.export_scene.gltf(
        filepath=str(pathlib.Path(out) / f"{name}.gltf"),
        export_format="GLTF_SEPARATE",
        use_selection=True,
        export_image_format="NONE",
        export_materials="EXPORT",
        export_normals=True,
        export_texcoords=True,
        export_tangents=False,
        export_cameras=False,
        export_lights=False,
        export_yup=True,
    )


def report(obj, name):
    mesh = obj.data
    lo, hi = bounds(mesh)
    size = hi - lo
    tris = triangles(mesh)
    assert tris <= BUDGET, f"{name}: {tris} triangles"
    assert abs(lo.z) < 1e-4 and abs(lo.x + hi.x) < 1e-3 and abs(lo.y + hi.y) < 1e-3, f"{name}: not on its foot"
    # The game's axes: x stays, y (up) is Blender's z, z (depth) is Blender's y.
    print(f"UNIT {json.dumps({'name': name, 'tris': tris, 'size': [round(size.x, 3), round(size.z, 3), round(size.y, 3)]})}")


def crate_stack(src, out, layers):
    crate = import_meshes(pathlib.Path(src) / "wooden_crate_02" / "wooden_crate_02_1k.gltf")
    decimate(crate, CRATE_TRIS)
    mesh = crate.data
    # The model's long side is Y (1.166 m), its width X and its height Z; lay the crates along X
    # or Z as the course goes, and stretch each to its place in the stack.
    lo, hi = bounds(mesh)
    mesh.transform(Matrix.Translation(Vector((-(lo.x + hi.x) / 2, -(lo.y + hi.y) / 2, -lo.z))))
    lo, hi = bounds(mesh)
    height = CRATE_LAYER / (hi.z - lo.z)
    parts = []
    off = CRATE_SPAN / 2 - (hi.x - lo.x) / 2
    for layer in range(layers):
        for side in (-1, 1):
            # Even layers lie along Y; odd ones are turned a quarter, so the stack is bonded.
            turn = math.pi / 2 if layer % 2 else 0.0
            shift = Vector((side * off, 0, 0)) if layer % 2 == 0 else Vector((0, side * off, 0))
            # A touch of yaw so no two courses are quite true.
            yaw = math.radians((-1.5, 2.0, -2.0, 1.0, 1.5, -1.0)[(layer * 2 + (side > 0)) % 6])
            matrix = Matrix.Translation(shift + Vector((0, 0, layer * CRATE_LAYER))) @ Matrix.Rotation(turn + yaw, 4, "Z") @ Matrix.Diagonal(Vector((1, 1, height, 1)))
            parts.append((mesh, matrix))
    unit = combine(parts, f"crate_stack_{layers}")
    ground(unit.data)
    single_material(unit, "crate_wood")
    name = f"crate_stack_{layers}"
    export(unit, out, name)
    report(unit, name)


def burlap_uvs(mesh, offset):
    """Box-projects `mesh` into burlap-tile UVs (one repeat per `BURLAP_TILE` metres), turned by
    `offset` (u, v, in repeats) so no two bags show the same weave."""
    if not mesh.uv_layers:
        mesh.uv_layers.new(name="UVMap")
    layer = mesh.uv_layers.active
    for poly in mesh.polygons:
        n = poly.normal
        a = max(range(3), key=lambda i: abs(n[i]))
        for loop in poly.loop_indices:
            p = mesh.vertices[mesh.loops[loop].vertex_index].co
            if a == 0:
                u, v = p.y, p.z
            elif a == 1:
                u, v = p.x, p.z
            else:
                u, v = p.x, p.y
            layer.data[loop].uv = (u / BURLAP_TILE + offset[0], v / BURLAP_TILE + offset[1])


def sandbags(src, out, courses):
    bag = import_meshes(pathlib.Path(src) / "cement_bag" / "cement_bag_1k.gltf")
    decimate(bag, BAG_TRIS)
    mesh = bag.data
    ground(mesh)
    lo, hi = bounds(mesh)
    # The sack lies long along Y: stretch it to a sandbag's course.
    mesh.transform(Matrix.Diagonal(Vector((BAG_WIDTH / (hi.x - lo.x), BAG_LENGTH / (hi.y - lo.y), BAG_COURSE / (hi.z - lo.z), 1.0))))
    rng = random.Random(7919 * courses)
    parts = []
    for course in range(courses):
        for side in (-1, 1):
            matrix = (
                Matrix.Translation(Vector((side * 0.25 + rng.uniform(-0.01, 0.01), (0.04 if course % 2 else -0.04) + rng.uniform(-0.015, 0.015), course * BAG_COURSE)))
                @ Matrix.Rotation(math.radians(rng.uniform(-2.5, 2.5)), 4, "Z")
                @ Matrix.Diagonal(Vector((rng.uniform(0.97, 1.03), rng.uniform(0.97, 1.03), rng.uniform(0.96, 1.04), 1.0)))
            )
            copy = mesh.copy()
            copy.transform(matrix)
            burlap_uvs(copy, (rng.random(), rng.random()))
            parts.append((copy, Matrix.Identity(4)))
    name = f"sandbags_{courses}"
    unit = combine(parts, name)
    ground(unit.data)
    single_material(unit, "burlap")
    export(unit, out, name)
    report(unit, name)


def headstone(scan, out, work):
    """Decimates a high-poly scan to about `HEADSTONE_TRIS` triangles, unwraps it, and bakes its
    normal and base colour from the scan, into PNGs under `work`. The scan is scaled to the
    Headstone's box. Needs Cycles."""
    high = import_meshes(scan)
    high.name = "high"
    ground(high.data)
    lo, hi = bounds(high.data)
    extent = hi - lo
    # Scans stand upright; scale uniformly to the height, then squeeze to fit the box.
    k = HEADSTONE_SIZE[2] / extent.z
    high.data.transform(Matrix.Diagonal(Vector((min(k, HEADSTONE_SIZE[0] / extent.x), min(k, HEADSTONE_SIZE[1] / extent.y), k, 1.0))))
    ground(high.data)

    low = high.copy()
    low.data = high.data.copy()
    low.name = "low"
    bpy.context.scene.collection.objects.link(low)
    decimate(low, HEADSTONE_TRIS)
    select_only([low])
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=math.radians(66), island_margin=0.003)
    bpy.ops.object.mode_set(mode="OBJECT")

    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 8
    scene.cycles.device = "CPU"
    scene.render.bake.margin = 8
    scene.render.bake.use_selected_to_active = True
    scene.render.bake.cage_extrusion = 0.02
    scene.render.bake.use_pass_direct = False
    scene.render.bake.use_pass_indirect = False
    scene.render.bake.use_pass_color = True
    work = pathlib.Path(work)
    work.mkdir(parents=True, exist_ok=True)
    for kind, bake_type, space, colour in (("normal", "NORMAL", "linear", (0.5, 0.5, 1.0, 1.0)), ("color", "DIFFUSE", "sRGB", (0.5, 0.5, 0.5, 1.0))):
        image = bpy.data.images.new(f"headstone_{kind}", 1024, 1024)
        image.generated_color = colour
        image.colorspace_settings.name = "Non-Color" if space == "linear" else "sRGB"
        mat = bpy.data.materials.new(f"bake_{kind}")
        mat.use_nodes = True
        node = mat.node_tree.nodes.new("ShaderNodeTexImage")
        node.image = image
        mat.node_tree.nodes.active = node
        low.data.materials.clear()
        low.data.materials.append(mat)
        select_only([high, low])
        bpy.context.view_layer.objects.active = low
        bpy.ops.object.bake(type=bake_type, normal_space="TANGENT", normal_r="POS_X", normal_g="POS_Y", normal_b="POS_Z")
        image.filepath_raw = str(work / f"headstone_{kind}.png")
        image.file_format = "PNG"
        image.save()
    bpy.data.objects.remove(high)
    unit = low
    ground(unit.data)
    single_material(unit, "headstone_stone")
    export(unit, out, "headstone")
    report(unit, "headstone")


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--src", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--only", choices=["crate", "sandbags", "headstone"])
    parser.add_argument("--headstone", help="a high-poly headstone scan to decimate and bake")
    parser.add_argument("--work", help="where the headstone's baked PNGs go")
    args = parser.parse_args(argv)
    out = pathlib.Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    if args.only in (None, "crate"):
        for layers in (2, 3):
            reset()
            crate_stack(args.src, out, layers)
    if args.only in (None, "sandbags"):
        for courses in (3, 5):
            reset()
            sandbags(args.src, out, courses)
    if args.headstone:
        reset()
        headstone(args.headstone, out, args.work or out)


main()
