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
from mathutils import Euler, Matrix, Vector

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


def export(obj, out, name, colours=False):
    """Writes `out/name.gltf` and `name.bin`, with no images (and the vertex colours if `colours`)."""
    select_only([obj])
    extra = {"export_vertex_color": "ACTIVE", "export_vertex_color_name": "Col", "export_all_vertex_colors": False} if colours else {}
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
        **extra,
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


# ---- The Sanatorium's kitbashes (#525): built from primitives, wearing material sets already in
# assets/materials. A unit is boxes and cylinders in Blender's axes (x length, y depth, z up), each
# face in one named material whose UV tile is its real-world size in metres.

# material set id -> its UV tile, metres (the `real-world size` in its manifest entry).
SET_TILE = {
    "rusty_metal_02": 1.0,
    "fabric_pattern_07": 0.4,
    "painted_metal_shutter": 2.0,
    "green_metal_rust": 1.0,
    "long_white_tiles": 1.27,
    "metal_plate_02": 2.0,
    "dark_wooden_planks": 2.0,
    "weathered_planks": 2.0,
    # The Archive's (#540).
    "wood_table_worn": 0.55,
    "dark_paneled_wood": 1.4,
    "brown_leather": 0.4,
    "paper006": 0.3,
    # Flat colours with no texture of their own (`build_props.py` gives each its factors).
    "lamp_shade": 1.0,
    "lamp_bulb": 1.0,
    "glass": 1.0,
    "ink": 1.0,
    # The Archive's stacks and cellar (#541).
    "candle_wax": 1.0,
    # The Refinery's Valve stand (#572): flat colours with no texture of their own.
    "hazard_yellow": 1.0,
    "hazard_black": 1.0,
    "gauge_white": 1.0,
    "valve_red": 1.0,
    "needle_red": 1.0,
}


class Kit:
    """Boxes and cylinders gathered into one mesh, a material set on each."""

    def __init__(self):
        self.bm = bmesh.new()
        self.uv = self.bm.loops.layers.uv.new("UVMap")
        self.sets = []

    def _faces_material(self, verts, material):
        """Gives the faces of `verts` (just made) their material, leaving their own UVs and normals."""
        if material not in self.sets:
            self.sets.append(material)
        index = self.sets.index(material)
        for face in {f for v in verts for f in v.link_faces}:
            face.material_index = index
            face.smooth = material == "paper006"

    def _faces(self, verts, material):
        """Gives the faces of `verts` (just made) their material and a box-projected UV."""
        if material not in self.sets:
            self.sets.append(material)
        index = self.sets.index(material)
        tile = SET_TILE[material]
        faces = {f for v in verts for f in v.link_faces}
        for face in faces:
            face.material_index = index
            face.smooth = False
            n = face.normal
            a = max(range(3), key=lambda i: abs(n[i]))
            for loop in face.loops:
                p = loop.vert.co
                u, v = ((p.y, p.z), (p.x, p.z), (p.x, p.y))[a]
                loop[self.uv].uv = (u / tile, v / tile)

    def box(self, centre, size, material, rot=(0.0, 0.0, 0.0)):
        """A box `size` (x, y, z) centred `centre`, turned by `rot` (Euler, radians, about x, y, z)."""
        matrix = Matrix.Translation(Vector(centre)) @ Euler(rot, "XYZ").to_matrix().to_4x4() @ Matrix.Diagonal(Vector((*size, 1.0)))
        made = bmesh.ops.create_cube(self.bm, size=1.0, matrix=matrix)
        self._faces(made["verts"], material)

    def cyl(self, centre, radius, length, axis, material, segments=10):
        """A cylinder of `radius` and `length` along `axis` ("x", "y" or "z"), centred `centre`."""
        along = {"x": Euler((0.0, math.pi / 2, 0.0)), "y": Euler((-math.pi / 2, 0.0, 0.0)), "z": Euler((0.0, 0.0, 0.0))}[axis]
        matrix = Matrix.Translation(Vector(centre)) @ along.to_matrix().to_4x4()
        made = bmesh.ops.create_cone(self.bm, cap_ends=True, cap_tris=False, segments=segments, radius1=radius, radius2=radius, depth=length, matrix=matrix)
        self._faces(made["verts"], material)

    def cone(self, centre, radius_low, radius_high, length, material, segments=12):
        """A cylinder along z whose foot has `radius_low` and top `radius_high` (a bucket, a candle), centred `centre`."""
        matrix = Matrix.Translation(Vector(centre))
        made = bmesh.ops.create_cone(self.bm, cap_ends=True, cap_tris=False, segments=segments, radius1=radius_low, radius2=radius_high, depth=length, matrix=matrix)
        self._faces(made["verts"], material)

    def finish(self, name, grounded=True):
        """The mesh object: its materials named for their sets, its foot's middle at the origin (or, ungrounded, its own origin)."""
        mesh = bpy.data.meshes.new(name)
        self.bm.to_mesh(mesh)
        self.bm.free()
        for material in self.sets:
            mat = bpy.data.materials.new(material)
            mat.use_nodes = True
            mesh.materials.append(mat)
        obj = bpy.data.objects.new(name, mesh)
        bpy.context.scene.collection.objects.link(obj)
        if grounded:
            ground(mesh)
        return obj


def gurney():
    k = Kit()
    steel, cloth = "rusty_metal_02", "fabric_pattern_07"
    for y in (-0.26, 0.26):
        k.cyl((0, y, 0.72), 0.022, 1.9, "x", steel)
    for x in (-0.88, 0.0, 0.88):
        k.cyl((x, 0, 0.72), 0.018, 0.56, "y", steel)
    for x in (-0.8, 0.8):
        for y in (-0.26, 0.26):
            k.cyl((x, y, 0.4), 0.02, 0.62, "z", steel)
            k.cyl((x, y, 0.07), 0.07, 0.04, "y", steel)
    k.box((0.1, 0, 0.8), (1.5, 0.58, 0.1), cloth)
    # The head end propped up on its rest.
    k.box((-0.78, 0, 0.9), (0.55, 0.56, 0.08), cloth, rot=(0.0, 0.4, 0.0))
    # A pull bar at the foot.
    k.cyl((0.95, 0, 0.9), 0.018, 0.5, "y", steel)
    return k.finish("gurney")


def dressing_trolley():
    k = Kit()
    steel, tray = "rusty_metal_02", "metal_plate_02"
    for x in (-0.4, 0.4):
        for y in (-0.23, 0.23):
            k.cyl((x, y, 0.5), 0.02, 0.84, "z", steel)
            k.cyl((x, y, 0.05), 0.05, 0.04, "y", steel)
    for z in (0.35, 0.85):
        k.box((0, 0, z), (0.9, 0.52, 0.03), tray)
        for y in (-0.255, 0.255):
            k.box((0, y, z + 0.03), (0.9, 0.02, 0.04), tray)
        for x in (-0.44, 0.44):
            k.box((x, 0, z + 0.03), (0.02, 0.52, 0.04), tray)
    k.cyl((-0.43, 0, 0.95), 0.018, 0.52, "y", steel)
    return k.finish("dressing_trolley")


def locker_bank():
    k = Kit()
    paint, steel = "painted_metal_shutter", "rusty_metal_02"
    k.box((0, 0, 0.9), (1.2, 0.44, 1.8), paint)
    # Doors on both faces, since the fitting doesn't turn a unit to face the room.
    for side in (-1, 1):
        for i in (-1, 0, 1):
            x = i * 0.4
            k.box((x, side * 0.225, 0.9), (0.37, 0.02, 1.72), paint)
            for row in range(4):
                k.box((x, side * 0.238, 1.5 + row * 0.06), (0.22, 0.01, 0.02), steel)
            k.box((x + 0.12, side * 0.24, 0.95), (0.03, 0.02, 0.12), steel)
    k.box((0, 0, 0.04), (1.22, 0.46, 0.08), steel)
    return k.finish("locker_bank")


def cot():
    k = Kit()
    wood, cloth = "dark_wooden_planks", "fabric_pattern_07"
    for x in (-0.6, 0.6):
        for y in (-0.3, 0.3):
            k.box((x, y, 0.425), (0.05, 0.05, 0.85), wood)
    for y in (-0.3, 0.3):
        k.box((0, y, 0.82), (1.2, 0.04, 0.04), wood)
        k.box((0, y, 0.4), (1.2, 0.04, 0.04), wood)
        for i in range(-3, 4):
            k.box((i * 0.15, y, 0.61), (0.02, 0.02, 0.4), wood)
    for x in (-0.6, 0.6):
        k.box((x, 0, 0.82), (0.04, 0.6, 0.04), wood)
    k.box((0, 0, 0.36), (1.12, 0.56, 0.1), cloth)
    return k.finish("cot")


def mortuary_slab():
    k = Kit()
    plate, steel = "metal_plate_02", "rusty_metal_02"
    k.box((0, 0, 0.925), (2.1, 0.75, 0.05), plate)
    for y in (-0.36, 0.36):
        k.box((0, y, 0.96), (2.1, 0.03, 0.04), plate)
    for x in (-1.035, 1.035):
        k.box((x, 0, 0.96), (0.03, 0.75, 0.04), plate)
    k.box((0, 0, 0.45), (0.5, 0.4, 0.85), steel)
    k.box((0, 0, 0.03), (1.0, 0.62, 0.06), steel)
    for x in (-0.7, 0.7):
        k.box((x, 0, 0.88), (0.08, 0.5, 0.05), steel)
    return k.finish("mortuary_slab")


def bath_tub():
    k = Kit()
    tile, steel = "long_white_tiles", "rusty_metal_02"
    for y in (-0.34, 0.34):
        k.box((0, y, 0.42), (1.7, 0.1, 0.55), tile)
        k.box((0, y, 0.7), (1.74, 0.14, 0.04), tile)
    for x in (-0.8, 0.8):
        k.box((x, 0, 0.42), (0.1, 0.58, 0.55), tile)
        k.box((x, 0, 0.7), (0.14, 0.8, 0.04), tile)
    k.box((0, 0, 0.18), (1.5, 0.6, 0.06), tile)
    for x in (-0.72, 0.72):
        for y in (-0.3, 0.3):
            k.cyl((x, y, 0.07), 0.05, 0.14, "z", steel, segments=8)
    return k.finish("bath_tub")


def rocking_horse():
    k = Kit()
    wood, cloth = "dark_wooden_planks", "fabric_pattern_07"
    radius, steps = 1.0, 8
    for y in (-0.17, 0.17):
        for i in range(steps):
            a0, a1 = math.radians(-32 + 64 * i / steps), math.radians(-32 + 64 * (i + 1) / steps)
            (x0, z0), (x1, z1) = ((radius * math.sin(a), radius * (1 - math.cos(a))) for a in (a0, a1))
            length = math.hypot(x1 - x0, z1 - z0) + 0.02
            k.box(((x0 + x1) / 2, y, (z0 + z1) / 2 + 0.025), (length, 0.04, 0.05), wood, rot=(0.0, -math.atan2(z1 - z0, x1 - x0), 0.0))
    for x in (-0.26, 0.26):
        for y in (-0.1, 0.1):
            k.box((x, y, 0.32), (0.06, 0.06, 0.4), wood)
    k.box((0, 0, 0.58), (0.74, 0.2, 0.22), wood)
    k.box((0, 0, 0.7), (0.3, 0.22, 0.05), cloth)
    k.box((0.38, 0, 0.78), (0.14, 0.14, 0.34), wood, rot=(0.0, -0.35, 0.0))
    k.box((0.5, 0, 0.93), (0.26, 0.13, 0.14), wood, rot=(0.0, 0.2, 0.0))
    k.box((-0.4, 0, 0.64), (0.08, 0.06, 0.28), wood, rot=(0.0, 0.5, 0.0))
    return k.finish("rocking_horse")


def ambulance():
    k = Kit()
    body, dark = "green_metal_rust", "rusty_metal_02"
    k.box((0, 0, 0.58), (4.9, 1.9, 0.24), dark)
    k.box((-0.95, 0, 1.42), (3.1, 2.0, 1.45), body)
    k.box((1.25, 0, 1.22), (1.3, 2.0, 1.05), body)
    k.box((2.2, 0, 0.98), (0.8, 1.9, 0.58), body)
    # Windows: a dark band down each side of the cab and the body, and the back doors' glass.
    for side in (-1, 1):
        k.box((1.2, side * 1.0, 1.45), (0.9, 0.03, 0.5), dark)
        k.box((-1.0, side * 1.0, 1.7), (2.2, 0.03, 0.3), dark)
    k.box((-2.5, 0, 1.45), (0.03, 1.8, 1.0), dark)
    k.box((1.9, 0, 1.2), (0.03, 1.7, 0.45), dark)
    k.box((2.42, 0, 0.7), (0.14, 1.95, 0.18), dark)
    k.box((-2.46, 0, 0.62), (0.12, 1.95, 0.18), dark)
    k.box((0.7, 0, 2.2), (0.3, 1.2, 0.12), dark)
    for x in (-1.6, 1.7):
        for y in (-0.95, 0.95):
            k.cyl((x, y, 0.4), 0.4, 0.24, "y", dark, segments=12)
    return k.finish("ambulance")


# ---- The Archive's Wheels (#538): a cast-iron three-spoke hand wheel with a brass hub, the fluted post it stands on and the
# chain that jams it when the Range is Sabotaged. Three separate units, because the wheel turns and the chain shows only
# when Sabotaged: all in the game's frame for a Wheel facing +z (Blender's -y), the post's origin the middle of its foot
# and the wheel's and the chain's the hub, 0.8 m up and 0.2 m out from the post (`HUB_AT` in `src/maps/stacks_look.rs`).

WHEEL_RADIUS = 0.35
WHEEL_POST_HEIGHT = 1.1
WHEEL_HUB_OUT = 0.2  # metres from the post's axis, along the way the Wheel faces


def wheel_post():
    k = Kit()
    iron, brass = "rusty_metal_02", "metal_plate_02"
    k.cyl((0, 0, 0.05), 0.15, 0.1, "z", iron, segments=12)
    # A fluted shaft: a core and eight thin ribs round it.
    k.cyl((0, 0, 0.1 + (WHEEL_POST_HEIGHT - 0.14) / 2), 0.055, WHEEL_POST_HEIGHT - 0.14, "z", iron, segments=10)
    for i in range(8):
        a = i * math.tau / 8
        k.box((0.062 * math.cos(a), 0.062 * math.sin(a), 0.1 + (WHEEL_POST_HEIGHT - 0.14) / 2), (0.02, 0.02, WHEEL_POST_HEIGHT - 0.14), iron, rot=(0.0, 0.0, a))
    k.cyl((0, 0, WHEEL_POST_HEIGHT - 0.02), 0.1, 0.04, "z", brass, segments=12)
    # The axle the hand wheel turns on, out of the post's face (Blender -y is the game's +z), to the hub's front.
    k.cyl((0, -(WHEEL_HUB_OUT + 0.045) / 2, 0.8), 0.025, WHEEL_HUB_OUT + 0.045, "y", iron)
    return k.finish("wheel_post")


def aisle_wheel():
    k = Kit()
    iron, brass = "rusty_metal_02", "metal_plate_02"
    # The rim: a ring of short bars in the x-z plane round the hub; three spokes and three rim handles.
    segments = 16
    chord = 2 * WHEEL_RADIUS * math.sin(math.pi / segments) + 0.01
    for i in range(segments):
        a = i * math.tau / segments
        k.box((WHEEL_RADIUS * math.cos(a), 0.0, WHEEL_RADIUS * math.sin(a)), (chord, 0.04, 0.04), iron, rot=(0.0, -(a + math.pi / 2), 0.0))
    for i in range(3):
        a = math.pi / 2 + i * math.tau / 3
        mid = (WHEEL_RADIUS - 0.04) / 2 + 0.02
        k.box((mid * math.cos(a), 0.0, mid * math.sin(a)), (WHEEL_RADIUS - 0.04, 0.035, 0.04), iron, rot=(0.0, -a, 0.0))
        k.cyl(((WHEEL_RADIUS + 0.01) * math.cos(a), -0.04, (WHEEL_RADIUS + 0.01) * math.sin(a)), 0.016, 0.1, "y", iron, segments=8)
    k.cyl((0, 0, 0), 0.06, 0.09, "y", brass, segments=12)
    return k.finish("aisle_wheel", grounded=False)


def wheel_chain():
    k = Kit()
    iron, brass = "rusty_metal_02", "metal_plate_02"
    # A ring of links woven through the spokes: in front of one, behind the next.
    for i in range(12):
        a = i * math.tau / 12
        weave = 0.02 if i % 2 == 0 else -0.02
        k.box((0.18 * math.cos(a), weave, 0.18 * math.sin(a)), (0.05, 0.022, 0.022), iron, rot=(0.0, -(a + math.pi / 2), 0.0))
    # The run down to a padlock on the post (the post is behind the wheel: Blender +y).
    for i in range(3):
        k.box((0.0, 0.04 * (i + 1), -0.22 - 0.05 * i), (0.022, 0.022, 0.05), iron)
    k.box((0.0, 0.13, -0.4), (0.07, 0.035, 0.09), brass)
    return k.finish("wheel_chain", grounded=False)


def wheel_report(obj, name):
    lo, hi = bounds(obj.data)
    tris = triangles(obj.data)
    assert tris <= BUDGET, f"{name}: {tris} triangles"
    print(f"UNIT {json.dumps({'name': name, 'tris': tris, 'size': [round(hi.x - lo.x, 3), round(hi.z - lo.z, 3), round(hi.y - lo.y, 3)]})}")


WHEEL_KITS = {"wheel_post": wheel_post, "aisle_wheel": aisle_wheel, "wheel_chain": wheel_chain}


# ---- The Refinery's Valve (#572): a stand, its hand wheel and its gauge needle. All three are in the stand's own frame with
# Blender's -y facing the approach (the game's +z). The stand's origin is the middle of its foot; the wheel's and the needle's
# are their pivots, so the game can turn them. `HUB_AT`, `GAUGE_AT`, `MAST_AT` and the measures below are repeated in
# `src/maps/fire_look.rs`: keep the two in step. The Valve lamp is not modelled: the game draws it, as it changes colour.

VALVE_RISER = 1.1
VALVE_WHEEL_RADIUS = 0.3
VALVE_HUB_AT = (0.0, 1.1, 0.16)  # game x, y, z
VALVE_GAUGE_AT = (-0.34, 1.0, 0.06)
VALVE_MAST_AT = (0.4, -0.12)  # game x, z
VALVE_MAST_HEIGHT = 2.4
VALVE_STRIPE = 0.2


def g(x, y, z):
    """A point in the game's axes (x right, y up, z towards the approach) in Blender's (x, y, z up)."""
    return (x, -z, y)


def valve_stand():
    k = Kit()
    iron, yellow, black, white = "rusty_metal_02", "hazard_yellow", "hazard_black", "gauge_white"
    # The foot and the riser, up to the wheel's height.
    k.cyl(g(0, 0.03, 0), 0.2, 0.06, "z", iron, segments=12)
    k.cyl(g(0, VALVE_RISER / 2, 0), 0.075, VALVE_RISER, "z", iron, segments=10)
    # The valve body at the top, and the spindle the wheel turns on, out of its face to the hub's back.
    k.box(g(0, VALVE_RISER - 0.05, 0), (0.2, 0.14, 0.2), iron)
    k.cyl(g(0, VALVE_HUB_AT[1], VALVE_HUB_AT[2] / 2 - 0.02), 0.022, VALVE_HUB_AT[2] + 0.04, "y", iron, segments=8)
    # The gauge: a stub off the riser, the case, a white face and a dark bezel ring.
    gx, gy, gz = VALVE_GAUGE_AT
    k.cyl(g(gx / 2, gy, 0), 0.02, abs(gx), "x", iron, segments=8)
    k.cyl(g(gx, gy, gz), 0.09, 0.05, "y", iron, segments=14)
    k.cyl(g(gx, gy, gz + 0.027), 0.082, 0.006, "y", white, segments=14)
    k.cyl(g(gx, gy, gz + 0.026), 0.095, 0.01, "y", black, segments=14)
    # The mast beside it: alternating hazard bands from the floor to 2.4 m, a collar and a cage round the lamp on top.
    mx, mz = VALVE_MAST_AT
    for i in range(round(VALVE_MAST_HEIGHT / VALVE_STRIPE)):
        k.box(g(mx, VALVE_STRIPE * (i + 0.5), mz), (0.1, VALVE_STRIPE, 0.1), yellow if i % 2 == 0 else black)
    k.cyl(g(mx, VALVE_MAST_HEIGHT + 0.03, mz), 0.07, 0.06, "z", iron, segments=10)
    for dx, dz in ((-0.1, 0), (0.1, 0), (0, -0.1), (0, 0.1)):
        k.box(g(mx + dx, VALVE_MAST_HEIGHT + 0.16, mz + dz), (0.016, 0.2, 0.016), iron)
    k.cyl(g(mx, VALVE_MAST_HEIGHT + 0.27, mz), 0.12, 0.02, "z", iron, segments=10)
    return k.finish("valve_stand", grounded=False)


def valve_wheel():
    k = Kit()
    red, iron = "valve_red", "rusty_metal_02"
    # An 8-segment rim in the game's x-y plane (Blender's x-z) round the hub, four spokes and a hub.
    segments = 8
    chord = 2 * VALVE_WHEEL_RADIUS * math.sin(math.pi / segments) + 0.012
    for i in range(segments):
        a = i * math.tau / segments + math.pi / segments
        k.box((VALVE_WHEEL_RADIUS * math.cos(a), 0.0, VALVE_WHEEL_RADIUS * math.sin(a)), (chord, 0.05, 0.05), red, rot=(0.0, -(a + math.pi / 2), 0.0))
    for i in range(4):
        a = i * math.tau / 4
        mid = (VALVE_WHEEL_RADIUS - 0.03) / 2
        k.box((mid * math.cos(a), 0.0, mid * math.sin(a)), (VALVE_WHEEL_RADIUS - 0.03, 0.035, 0.04), red, rot=(0.0, -a, 0.0))
    k.cyl((0, 0, 0), 0.05, 0.08, "y", iron, segments=10)
    return k.finish("valve_wheel", grounded=False)


def valve_needle():
    k = Kit()
    red = "needle_red"
    # Pointing up (the game's +y) from its pivot at the gauge's middle; it turns about the depth axis.
    k.box((0, 0, 0.035), (0.012, 0.006, 0.075), red)
    k.cyl((0, 0, 0), 0.012, 0.01, "y", red, segments=8)
    return k.finish("valve_needle", grounded=False)


VALVE_KITS = {"valve_stand": valve_stand, "valve_wheel": valve_wheel, "valve_needle": valve_needle}


# ---- The Archive's book rows (#537): the shelf fill of every Bookcase and fixed stack. One mesh a
# face (front and top faces of the spines only, no backs, no sides), 6 shelves of about 50 spines,
# each spine tinted per vertex from a 12-colour library palette over the `book_pattern` cloth, folios
# (taller, wider, darker) on the bottom shelf. The unit's origin is the middle of a stack's foot; its
# face looks along +z in the game (Blender's -y) at the carcass face, 0.6 m out. Three variants, one mirrored
# and one offset, so neighbouring stacks never match; the other face of a stack is the same mesh
# turned half a revolution.

BOOK_LENGTH = 5.0
BOOK_FACE = 0.6 + 0.012  # a hair proud of the carcass's face
BOOK_DEPTH = 0.24
BOOK_SHELVES = 6
BOOK_FOOT = 0.04 + 0.1  # the carcass stands on its castors; the lowest board is 0.1 above it
BOOK_PITCH = 0.46
BOOK_BOARD = 0.04
BOOK_END = 0.1
BOOK_TILE = 0.3  # `book_pattern`'s swatch, metres

# The library palette, sRGB: oxblood, bottle green, navy, tan, black and seven more (each a leather or cloth).
BOOK_PALETTE = [
    (0.42, 0.09, 0.09), (0.10, 0.30, 0.17), (0.11, 0.17, 0.38), (0.72, 0.58, 0.38),
    (0.09, 0.08, 0.075), (0.40, 0.24, 0.13), (0.68, 0.48, 0.18), (0.26, 0.36, 0.48),
    (0.36, 0.07, 0.16), (0.38, 0.40, 0.18), (0.78, 0.7, 0.55), (0.55, 0.26, 0.12),
]
# (seed, mirrored, offset along x in metres)
BOOK_VARIANTS = {"a": (11, False, 0.0), "b": (23, True, 0.0), "c": (37, False, 0.17)}


def srgb_to_linear(c):
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def book_rows(variant):
    seed, mirrored, offset = BOOK_VARIANTS[variant]
    rng = random.Random(seed)
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UVMap")
    colour = bm.loops.layers.float_color.new("Col")

    def quad(corners, normal, rgb, uvs):
        verts = [bm.verts.new(c) for c in corners]
        face = bm.faces.new(verts)
        face.normal_update()
        if face.normal.dot(Vector(normal)) < 0:
            face.normal_flip()
        for loop, uv_at in zip(face.loops, uvs):
            loop[uv].uv = uv_at
            loop[colour] = (*rgb, 1.0)

    for shelf in range(BOOK_SHELVES):
        base = BOOK_FOOT + shelf * BOOK_PITCH + BOOK_BOARD
        folio = shelf == 0
        x = -BOOK_LENGTH / 2 + BOOK_END + offset % 0.3
        while True:
            width = rng.uniform(0.1, 0.15) if folio else rng.uniform(0.055, 0.13)
            height = rng.uniform(0.40, 0.44) if folio else rng.uniform(0.26, 0.38)
            if x + width > BOOK_LENGTH / 2 - BOOK_END:
                break
            pick = BOOK_PALETTE[rng.randrange(len(BOOK_PALETTE))]
            shade = rng.uniform(0.8, 1.15) * (0.7 if folio else 1.0)
            rgb = tuple(srgb_to_linear(min(1.0, c * shade)) for c in pick)
            x0, x1 = (-(x + width), -x) if mirrored else (x, x + width)
            front = -BOOK_FACE  # Blender's -y is the game's +z
            top = base + height
            quad([(x0, front, base), (x1, front, base), (x1, front, top), (x0, front, top)], (0, -1, 0), rgb,
                 [(x0 / BOOK_TILE, base / BOOK_TILE), (x1 / BOOK_TILE, base / BOOK_TILE), (x1 / BOOK_TILE, top / BOOK_TILE), (x0 / BOOK_TILE, top / BOOK_TILE)])
            back = front + BOOK_DEPTH
            quad([(x0, front, top), (x1, front, top), (x1, back, top), (x0, back, top)], (0, 0, 1), rgb,
                 [(x0 / BOOK_TILE, front / BOOK_TILE), (x1 / BOOK_TILE, front / BOOK_TILE), (x1 / BOOK_TILE, back / BOOK_TILE), (x0 / BOOK_TILE, back / BOOK_TILE)])
            # Spines are shoulder to shoulder, with the odd gap where a book is out.
            x += width + (rng.uniform(0.0, 0.01) if rng.random() < 0.9 else rng.uniform(0.05, 0.1))
    mesh = bpy.data.meshes.new(f"book_rows_{variant}")
    bm.to_mesh(mesh)
    bm.free()
    mesh.color_attributes.active_color = mesh.color_attributes["Col"]
    mesh.color_attributes.render_color_index = mesh.color_attributes.find("Col")
    mat = bpy.data.materials.new("book_pattern")
    mat.use_nodes = True
    mesh.materials.append(mat)
    for poly in mesh.polygons:
        poly.use_smooth = False
    obj = bpy.data.objects.new(f"book_rows_{variant}", mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def book_rows_report(obj, name):
    lo, hi = bounds(obj.data)
    tris = triangles(obj.data)
    assert tris <= BUDGET, f"{name}: {tris} triangles"
    print(f"UNIT {json.dumps({'name': name, 'tris': tris, 'size': [round(hi.x - lo.x, 3), round(hi.z - lo.z, 3), round(hi.y - lo.y, 3)]})}")


# ---- The Archive's reading-level kitbashes (#540): the card catalogue, the issue desk, the reading lamp, the lectern, the theatre's
# tiers, the projector, the display case and the globe. Each wears material sets already under assets/materials (and a few flat
# colours: the lamp's glass, the case's glass, the globe's ink), each faces +z in the game (Blender's -y), and each is one mesh of at
# most 2k triangles on its foot (the globe, on its stand's foot).

def card_catalogue():
    """One 2.0 x 1.5 x 1.2 m run of card drawers, drawers on both faces (the fitting lays four of them as a bank)."""
    k = Kit()
    wood, panel, brass = "wood_table_worn", "dark_paneled_wood", "metal_plate_02"
    k.box((0, 0, 0.04), (2.0, 1.14, 0.08), panel)
    k.box((0, 0, 0.08 + 0.55), (1.96, 1.14, 1.1), wood)
    k.box((0, 0, 1.44), (2.0, 1.2, 0.12), panel)
    cols, rows = 4, 5
    for side in (-1, 1):
        for i in range(cols):
            for j in range(rows):
                x = (i - (cols - 1) / 2) * 0.48
                z = 0.2 + j * 0.2
                k.box((x, side * 0.575, z), (0.44, 0.012, 0.17), panel)
                k.box((x, side * 0.588, z - 0.01), (0.1, 0.014, 0.02), brass)
                k.box((x, side * 0.588, z + 0.045), (0.12, 0.012, 0.05), brass)
    return k.finish("card_catalogue")


def issue_desk():
    """The Reading Room's issue desk: an octagonal counter 6 m across its flats (the Prop's box is 6 x 6), its clerks' well inside, a
    brass rail round its top and a post at its middle."""
    k = Kit()
    wood, panel, brass = "wood_table_worn", "dark_paneled_wood", "metal_plate_02"
    flat = 6.0 / 2
    side = 2 * flat * math.tan(math.pi / 8)
    for i in range(8):
        a = i * math.pi / 4
        # A panel whose face is `flat` from the middle, turned to face out along `a`.
        for radius, length, height, thick, material, z in ((flat - 0.3, side + 0.02, 1.0, 0.6, panel, 0.5), (flat - 0.35, side + 0.2, 0.07, 0.7, wood, 1.035)):
            c = (radius * math.cos(a), radius * math.sin(a), z)
            k.box(c, (thick, length, height), material, rot=(0.0, 0.0, a))
        # A brass rail along the top's outer edge, and a post at each corner.
        c = ((flat - 0.04) * math.cos(a), (flat - 0.04) * math.sin(a), 1.12)
        k.box(c, (0.03, side - 0.1, 0.03), brass, rot=(0.0, 0.0, a))
        corner = a + math.pi / 8
        r = flat / math.cos(math.pi / 8) - 0.08
        k.cyl((r * math.cos(corner), r * math.sin(corner), 0.55), 0.03, 1.1, "z", brass, segments=8)
    # The clerks' well: a low inner desk ring, then the post at the middle (a brass lamp standard) up to the unit's 1.6 m.
    k.cyl((0, 0, 0.05), 1.2, 0.1, "z", wood, segments=8)
    k.cyl((0, 0, 0.8), 0.14, 1.6, "z", wood, segments=8)
    k.cyl((0, 0, 1.55), 0.18, 0.1, "z", brass, segments=8)
    return k.finish("issue_desk")


def reading_lamp():
    """A banker's lamp: a brass base and stem, a green glass shade open underneath and a bare bulb in it. Faces +z; 0.3 x 0.44 x 0.25 m."""
    k = Kit()
    brass, shade, bulb = "metal_plate_02", "lamp_shade", "lamp_bulb"
    k.box((0, 0, 0.015), (0.2, 0.25, 0.03), brass)
    k.cyl((0, 0, 0.17), 0.014, 0.28, "z", brass, segments=8)
    k.box((0, 0, 0.32), (0.05, 0.05, 0.04), brass)
    # The shade: a roof and two sloping sides over the bulb, 0.3 long.
    k.box((0, 0, 0.425), (0.3, 0.1, 0.012), shade)
    for side in (-1, 1):
        k.box((0, side * 0.075, 0.385), (0.3, 0.012, 0.09), shade, rot=(side * 0.45, 0.0, 0.0))
    k.cyl((0, 0, 0.35), 0.022, 0.26, "x", bulb, segments=8)
    return k.finish("reading_lamp")


def lectern():
    """A reading lectern: a panelled pedestal, a sloped desk with a brass ledge. 0.7 x 1.2 x 0.6 m, its desk facing +z."""
    k = Kit()
    wood, panel, brass = "wood_table_worn", "dark_paneled_wood", "metal_plate_02"
    k.box((0, 0, 0.04), (0.6, 0.5, 0.08), panel)
    k.box((0, 0, 0.5), (0.4, 0.3, 0.84), wood)
    for side in (-1, 1):
        k.box((side * 0.215, 0, 0.5), (0.03, 0.32, 0.8), panel)
    k.box((0, 0, 1.0), (0.7, 0.06, 0.2), wood)
    # The desk: tipped towards the reader (the front is Blender -y), its ledge at the low edge.
    k.box((0, 0, 1.105), (0.7, 0.6, 0.04), panel, rot=(-0.4, 0.0, 0.0))
    k.box((0, -0.27, 1.02), (0.7, 0.03, 0.05), brass, rot=(-0.4, 0.0, 0.0))
    k.box((0, 0.27, 1.19), (0.7, 0.05, 0.02), wood)
    return k.finish("lectern")


def theatre_bench():
    """The Lecture Theatre's tiers, seven leather benches down each aisle and a panelled front: one 10 x 5.4 x 14 m unit, the solid
    bank inset 0.5 m from its box, the benches standing out to the box's edge."""
    k = Kit()
    wood, panel, leather, brass = "wood_table_worn", "dark_paneled_wood", "brown_leather", "metal_plate_02"
    w, h, d = 10.0, 5.4, 14.0
    inset = 0.5
    k.box((0, 0, h / 2), (w - 2 * inset, d - 2 * inset, h), panel)
    # The dais face (game +z, Blender -y): pilasters and a brass rail at the foot of the bank, in the 0.5 m beside the face.
    for i in range(5):
        k.box((-4.0 + i * 2.0, -(d / 2 - inset / 2), h / 2), (0.3, inset, h), wood)
    k.box((0, -(d / 2 - 0.15), 0.9), (w - 2 * inset, 0.06, 0.06), brass)
    # Seven benches down each side, each 2 m of the aisle's length and rising with the rake (4 m over its 14).
    for side in (-1, 1):
        x = side * (w / 2 - inset / 2)
        for row in range(7):
            # Front row nearest the dais (game +z, Blender -y): its centre 1 m back, then every 2 m.
            y = -(d / 2 - 1.0 - 2.0 * row)
            rise = 2.0 * row * 4.0 / 14.0
            k.box((x, y, rise + 0.43), (inset - 0.04, 1.8, 0.08), leather)
            k.box((side * (w / 2 - 0.03), y, rise + 0.7), (0.06, 1.8, 0.5), leather)
            for yy in (y - 0.8, y + 0.8):
                k.box((x, yy, rise + 0.2), (inset - 0.1, 0.06, 0.4), wood)
    return k.finish("theatre_bench")


def projector():
    """A lantern-slide projector on a small trolley: 0.9 x 1.3 x 0.6 m, its lens towards +x (the game's +x)."""
    k = Kit()
    wood, brass, iron = "wood_table_worn", "metal_plate_02", "rusty_metal_02"
    for x in (-0.38, 0.38):
        for y in (-0.22, 0.22):
            k.cyl((x, y, 0.45), 0.025, 0.9, "z", wood, segments=8)
            k.cyl((x, y, 0.04), 0.04, 0.03, "y", iron, segments=8)
    k.box((0, 0, 0.92), (0.88, 0.58, 0.04), wood)
    k.box((0, 0, 0.45), (0.8, 0.5, 0.03), wood)
    # The lamp house, the bellows and the lens.
    k.box((-0.2, 0, 1.12), (0.36, 0.34, 0.36), iron)
    k.cyl((-0.2, 0, 1.35), 0.07, 0.1, "z", iron, segments=8)
    k.box((0.1, 0, 1.12), (0.26, 0.24, 0.24), brass)
    k.cyl((0.35, 0, 1.12), 0.1, 0.2, "x", brass, segments=10)
    k.box((0.0, 0.0, 1.28), (0.9, 0.04, 0.04), iron)
    return k.finish("projector")


def display_case():
    """A glass-topped display case on turned legs, a few open books and loose pages under the glass: 1.2 x 1.0 x 0.7 m."""
    k = Kit()
    wood, panel, paper, glass, brass = "wood_table_worn", "dark_paneled_wood", "paper006", "glass", "metal_plate_02"
    for x in (-0.55, 0.55):
        for y in (-0.3, 0.3):
            k.cyl((x, y, 0.36), 0.04, 0.72, "z", wood, segments=8)
    k.box((0, 0, 0.74), (1.2, 0.7, 0.05), panel)
    # The case's frame: a brass rim and four corner posts round the glass box.
    for side in (-1, 1):
        k.box((0, side * 0.33, 0.78), (1.2, 0.04, 0.04), brass)
        k.box((side * 0.58, 0, 0.78), (0.04, 0.7, 0.04), brass)
        k.box((0, side * 0.33, 1.0), (1.2, 0.04, 0.04), brass)
        k.box((side * 0.58, 0, 1.0), (0.04, 0.7, 0.04), brass)
    for x in (-0.58, 0.58):
        for y in (-0.33, 0.33):
            k.box((x, y, 0.9), (0.04, 0.04, 0.2), brass)
    k.box((0, 0, 0.99), (1.14, 0.64, 0.012), glass)
    for side in (-1, 1):
        k.box((0, side * 0.33, 0.89), (1.14, 0.012, 0.2), glass)
        k.box((side * 0.58, 0, 0.89), (0.012, 0.64, 0.2), glass)
    # Under the glass: two open books and a loose sheet.
    k.box((-0.3, 0, 0.8), (0.4, 0.28, 0.03), paper)
    k.box((-0.3, 0, 0.82), (0.36, 0.25, 0.012), paper, rot=(0.0, 0.0, 0.1))
    k.box((0.25, 0.05, 0.8), (0.3, 0.22, 0.03), paper, rot=(0.0, 0.0, -0.2))
    return k.finish("display_case")


GLOBE_RADIUS = 0.3
GLOBE_AXIS = math.radians(23.5)
GLOBE_CENTRE = 0.93  # the sphere's middle over the stand's foot


def globe():
    """The Map Room's globe: a 0.6 m sepia Paper006 sphere tilted on its axis with a lat/long graticule laid over it in ink (no CC0
    globe exists, #514; the graticule is thin quads on the sphere, with no continents), in a brass meridian ring on a turned stand.
    0.7 x 1.25 x 0.7 m."""
    k = Kit()
    wood, brass, paper, ink = "wood_table_worn", "metal_plate_02", "paper006", "ink"
    tilt = Matrix.Rotation(GLOBE_AXIS, 4, "Y")
    centre = Vector((0, 0, GLOBE_CENTRE))
    segs, rings = 24, 12
    made = bmesh.ops.create_uvsphere(k.bm, u_segments=segs, v_segments=rings, radius=GLOBE_RADIUS, matrix=Matrix.Translation(centre) @ tilt, calc_uvs=True)
    for face in {f for v in made["verts"] for f in v.link_faces}:
        for loop in face.loops:
            u, v = loop[k.uv].uv
            loop[k.uv].uv = (u * 6.0, v * 3.0)
    k._faces_material(made["verts"], paper)

    def on_sphere(lat, lon, r):
        p = Vector((r * math.cos(lat) * math.cos(lon), r * math.cos(lat) * math.sin(lon), r * math.sin(lat)))
        return centre + tilt @ p

    def strip(a, b, width, material):
        """A thin quad along the sphere's surface between two (lat, lon) points, `width` wide, a hair proud."""
        pa, pb = on_sphere(*a, GLOBE_RADIUS + 0.002), on_sphere(*b, GLOBE_RADIUS + 0.002)
        mid = (pa + pb) / 2
        normal = (mid - centre).normalized()
        along = pb - pa
        side = along.cross(normal).normalized() * width / 2
        verts = [k.bm.verts.new(c) for c in (pa - side, pb - side, pb + side, pa + side)]
        face = k.bm.faces.new(verts)
        face.normal_update()
        if face.normal.dot(normal) < 0:
            face.normal_flip()
        k._faces_material(verts, material)

    steps = 24
    for lat_deg in (-60, -30, 0, 30, 60):
        lat = math.radians(lat_deg)
        for s in range(steps):
            strip((lat, math.tau * s / steps), (lat, math.tau * (s + 1) / steps), 0.006, ink)
    for lon_deg in range(0, 360, 30):
        lon = math.radians(lon_deg)
        for s in range(12):
            strip((math.radians(-90 + 15 * s), lon), (math.radians(-90 + 15 * (s + 1)), lon), 0.006, ink)
    # The brass meridian ring (a ring of short boxes in the x-z plane round the sphere), tipped to the axis, with the axle's caps.
    ring_r = GLOBE_RADIUS + 0.04
    segments = 20
    chord = 2 * ring_r * math.sin(math.pi / segments) + 0.006
    rot = tilt.to_euler()
    for i in range(segments):
        a = i * math.tau / segments
        local = Vector((ring_r * math.cos(a), 0.0, ring_r * math.sin(a)))
        k.box(centre + tilt @ local, (chord, 0.03, 0.02), brass, rot=(rot.x, rot.y - a - math.pi / 2, rot.z))
    for sign in (-1, 1):
        k.cyl(centre + tilt @ Vector((0, 0, sign * (ring_r + 0.01))), 0.025, 0.04, "z", brass, segments=8)
    # The stand: a turned post with a collar, a round foot, and a brass arm up to the ring's lowest point.
    k.cyl((0, 0, 0.03), 0.3, 0.06, "z", wood, segments=12)
    k.cyl((0, 0, 0.35), 0.035, 0.6, "z", wood, segments=8)
    k.cyl((0, 0, 0.5), 0.065, 0.08, "z", wood, segments=8)
    k.cyl((0, 0, 0.68), 0.05, 0.06, "z", wood, segments=8)
    k.cyl((0, 0, 0.74), 0.03, 0.16, "z", brass, segments=8)
    return k.finish("globe")


# ---- The Archive's stacks and cellar kitbashes (#541): the book trolley, the book pile, the book press, the guillotine, the safe, the
# deed boxes, the pump engine, the bucket and the candles. As the reading level's: material sets already under assets/materials (and
# the candles' wax, a flat factor), facing +z in the game (Blender's -y), one mesh of at most 2k triangles on its foot.

def book_trolley():
    """A library book trolley: a wooden frame on four casters, three book shelves and a push handle at one end. About 1.0 x 1.2 x 0.55 m."""
    k = Kit()
    wood, panel, leather, paper, iron = "wood_table_worn", "dark_paneled_wood", "brown_leather", "paper006", "rusty_metal_02"
    for x in (-0.46, 0.46):
        for y in (-0.22, 0.22):
            k.box((x, y, 0.47), (0.05, 0.05, 0.84), wood)
            k.cyl((x, y, 0.05), 0.05, 0.03, "y", iron, segments=8)
    # The handle end (-x) stands taller, with a bar across.
    for y in (-0.22, 0.22):
        k.box((-0.46, y, 1.0), (0.05, 0.05, 0.4), wood)
    k.cyl((-0.46, 0, 1.18), 0.02, 0.5, "y", iron, segments=8)
    for z in (0.14, 0.52, 0.86):
        k.box((0, 0, z), (0.96, 0.5, 0.03), panel)
        for y in (-0.245, 0.245):
            k.box((0, y, z + 0.04), (0.96, 0.02, 0.06), panel)
    # Books on each shelf: standing rows of leather and cloth spines, leaning at the end.
    for n, z in enumerate((0.14, 0.52, 0.86)):
        x = -0.38
        for i in range(9):
            w = 0.06 + 0.01 * ((i + n) % 3)
            h = 0.2 + 0.03 * ((i * 2 + n) % 3)
            k.box((x + w / 2, 0.0, z + 0.015 + h / 2), (w, 0.2, h), leather if (i + n) % 2 == 0 else paper, rot=(0.0, 0.06 * (1 if i == 8 else 0), 0.0))
            x += w + 0.005
    return k.finish("book_trolley")


def book_pile():
    """A heap of books on the floor: three stacks, each layer turned a little off the one below. About 0.9 x 0.8 x 0.7 m."""
    k = Kit()
    leather, paper = "brown_leather", "paper006"
    stacks = [((-0.22, 0.0), 8, 0.1), ((0.2, -0.08), 6, 0.35), ((0.05, 0.2), 4, -0.6)]
    for (cx, cy), layers, turn in stacks:
        z = 0.0
        for i in range(layers):
            h = 0.075 + 0.015 * ((i * 3 + layers) % 3)
            w, d = 0.42 - 0.02 * (i % 3), 0.3 - 0.015 * (i % 2)
            yaw = turn + 0.25 * ((i * 5) % 4 - 1.5) * 0.4
            k.box((cx + 0.015 * (i % 3), cy, z + h / 2), (w, d, h), leather if i % 2 == 0 else paper, rot=(0.0, 0.0, yaw))
            z += h
    # A few loose books slid down the side.
    k.box((0.38, 0.12, 0.04), (0.3, 0.22, 0.06), leather, rot=(0.0, 0.0, 0.5))
    k.box((-0.3, 0.2, 0.035), (0.26, 0.2, 0.05), paper, rot=(0.0, 0.0, -0.4))
    return k.finish("book_pile")


def book_press():
    """A standing screw press: a timber base, two uprights and a cross-head, an iron screw with a lever through its head over a platen
    and a stack of folded sheets. About 1.3 x 1.8 x 0.8 m, its screw facing up."""
    k = Kit()
    wood, panel, iron, paper, brass = "wood_table_worn", "dark_paneled_wood", "rusty_metal_02", "paper006", "metal_plate_02"
    k.box((0, 0, 0.12), (1.3, 0.8, 0.24), panel)
    for x in (-0.5, 0.5):
        k.box((x, 0, 0.9), (0.16, 0.5, 1.5), wood)
        k.box((x, 0, 0.3), (0.3, 0.6, 0.12), wood)
    k.box((0, 0, 1.55), (1.16, 0.5, 0.22), panel)
    k.cyl((0, 0, 1.2), 0.07, 0.9, "z", iron, segments=10)
    k.cyl((0, 0, 1.55), 0.11, 0.26, "z", iron, segments=10)
    # The lever through the screw's head.
    k.cyl((0, 0, 1.72), 0.025, 1.3, "x", iron, segments=8)
    for x in (-0.66, 0.66):
        k.cyl((x, 0, 1.72), 0.05, 0.08, "x", brass, segments=8)
    k.box((0, 0, 0.88), (0.7, 0.5, 0.06), wood)
    k.box((0, 0, 0.45), (0.62, 0.42, 0.42), paper)
    k.box((0, 0, 0.7), (0.66, 0.46, 0.06), wood)
    return k.finish("book_press")


def guillotine():
    """A paper guillotine on a work bench: a bed, a stack of paper, a long blade arm pivoting at one end and a clamp. About 1.2 x 1.2 x 0.8 m."""
    k = Kit()
    wood, panel, iron, paper, brass = "wood_table_worn", "dark_paneled_wood", "rusty_metal_02", "paper006", "metal_plate_02"
    for x in (-0.5, 0.5):
        for y in (-0.3, 0.3):
            k.box((x, y, 0.4), (0.08, 0.08, 0.8), wood)
    k.box((0, 0, 0.82), (1.2, 0.8, 0.06), panel)
    k.box((0, 0, 0.45), (1.0, 0.7, 0.04), wood)
    # The cutting bed and the paper on it.
    k.box((0, 0.05, 0.9), (1.0, 0.5, 0.1), iron)
    k.box((0, 0.0, 0.98), (0.5, 0.36, 0.06), paper)
    # The blade arm: a pivot post at the left, a long wooden arm swung half up, the blade along its foot.
    k.box((-0.52, 0.28, 1.0), (0.1, 0.1, 0.4), iron)
    k.cyl((-0.52, 0.28, 1.2), 0.04, 0.16, "y", brass, segments=8)
    k.box((0.0, 0.28, 1.2), (1.2, 0.07, 0.07), wood, rot=(0.0, -0.18, 0.0))
    k.box((0.1, 0.22, 1.12), (0.7, 0.02, 0.1), iron, rot=(0.0, -0.18, 0.0))
    k.cyl((0.58, 0.28, 1.12), 0.03, 0.25, "z", wood, segments=8)
    # The clamp wheel over the paper.
    k.cyl((0.0, 0.0, 1.07), 0.025, 0.2, "z", iron, segments=8)
    k.cyl((0.0, 0.0, 1.2), 0.07, 0.03, "z", brass, segments=10)
    return k.finish("guillotine")


def safe():
    """An iron safe: a squat riveted body, a heavy door with a brass wheel handle and dial, hinges and plinth feet. About 0.9 x 1.5 x 0.8 m."""
    k = Kit()
    iron, brass, panel = "rusty_metal_02", "metal_plate_02", "dark_paneled_wood"
    k.box((0, 0, 0.06), (0.9, 0.8, 0.12), iron)
    k.box((0, 0, 0.8), (0.86, 0.76, 1.36), iron)
    k.box((0, 0, 1.46), (0.92, 0.82, 0.08), iron)
    # The door's raised panel, on its face (the front is Blender -y).
    k.box((0, -0.395, 0.8), (0.7, 0.03, 1.1), panel)
    k.box((0, -0.41, 0.8), (0.6, 0.02, 1.0), iron)
    for z in (0.35, 1.25):
        k.box((-0.4, -0.4, z), (0.06, 0.06, 0.2), brass)
    # The wheel handle: a hub, a rim and four spokes; and a dial above it.
    k.cyl((0, -0.44, 0.78), 0.05, 0.08, "y", brass, segments=10)
    k.cyl((0, -0.44, 0.78), 0.16, 0.025, "y", brass, segments=14)
    for a in range(4):
        ang = a * math.pi / 4
        k.box((0, -0.44, 0.78), (0.3, 0.025, 0.02), brass, rot=(0.0, ang, 0.0))
    k.cyl((0, -0.425, 1.12), 0.07, 0.03, "y", brass, segments=12)
    for x in (-0.42, 0.42):
        k.box((x, 0, 0.8), (0.04, 0.78, 1.38), iron)
    return k.finish("safe")


def deed_boxes():
    """Japanned tin deed boxes stacked three high, three to a tier on a plank base, each with a brass hasp and a paper label. About 1.0 x 1.0 x 0.6 m."""
    k = Kit()
    tin, brass, paper, wood = "green_metal_rust", "metal_plate_02", "paper006", "wood_table_worn"
    k.box((0, 0, 0.03), (1.0, 0.6, 0.06), wood)
    for tier in range(3):
        z = 0.06 + tier * 0.31
        for i in (-1, 0, 1):
            x = i * 0.33 + (0.02 if tier == 1 else 0.0)
            k.box((x, 0, z + 0.15), (0.31, 0.5, 0.29), tin)
            k.box((x, 0, z + 0.3), (0.32, 0.51, 0.02), tin)
            k.box((x, -0.255, z + 0.2), (0.06, 0.012, 0.07), brass)
            k.box((x, -0.26, z + 0.12), (0.2, 0.01, 0.07), paper)
            k.box((x, 0.0, z + 0.33), (0.1, 0.04, 0.03), brass)
    return k.finish("deed_boxes")


def pump_engine():
    """A cellar pump engine: a cast bed with a horizontal steam cylinder, a crosshead and connecting rod to a big spoked flywheel, a vertical
    boiler stack and a standpipe. About 2.2 x 2.0 x 1.4 m, the flywheel on its +x end."""
    k = Kit()
    green, brass, iron = "green_metal_rust", "metal_plate_02", "rusty_metal_02"
    k.box((0, 0, 0.12), (2.2, 1.0, 0.24), iron)
    k.box((-0.3, 0, 0.45), (1.1, 0.7, 0.42), green)
    k.cyl((-0.35, 0, 0.9), 0.3, 1.1, "x", green, segments=12)
    k.cyl((-0.95, 0, 0.9), 0.34, 0.08, "x", brass, segments=12)
    k.cyl((0.6, 0, 0.9), 0.05, 0.9, "x", iron, segments=8)
    k.box((0.2, 0, 0.9), (0.2, 0.2, 0.2), brass)
    # The flywheel (on its axle past the +x end): a rim, a hub and spokes.
    k.cyl((0.95, 0.0, 1.0), 0.08, 0.4, "y", iron, segments=10)
    k.cyl((0.95, -0.08, 1.0), 0.78, 0.07, "y", green, segments=16)
    k.cyl((0.95, -0.08, 1.0), 0.62, 0.03, "y", iron, segments=16)
    for a in range(6):
        ang = a * math.pi / 3
        k.box((0.95, -0.08, 1.0), (1.2, 0.04, 0.05), iron, rot=(0.0, ang, 0.0))
    k.cyl((0.95, -0.08, 1.0), 0.12, 0.1, "y", brass, segments=10)
    # The boiler stack and standpipe, and a gauge.
    k.cyl((-0.7, 0.3, 1.4), 0.12, 0.8, "z", green, segments=10)
    k.cyl((-0.7, 0.3, 1.82), 0.17, 0.08, "z", iron, segments=10)
    k.cyl((-0.05, 0.38, 1.1), 0.05, 0.7, "z", brass, segments=8)
    k.cyl((-0.05, 0.38, 1.5), 0.1, 0.05, "y", brass, segments=10)
    for x in (-0.8, 0.0):
        k.box((x, 0.0, 0.24), (0.14, 0.9, 0.12), iron)
    return k.finish("pump_engine")


def bucket():
    """A galvanised bucket, slightly tapered, with a wire bail. About 0.4 x 0.36 x 0.4 m."""
    k = Kit()
    zinc = "metal_plate_02"
    k.cone((0, 0, 0.18), 0.15, 0.2, 0.36, zinc, segments=14)
    k.cone((0, 0, 0.36), 0.205, 0.205, 0.025, zinc, segments=14)
    for i in range(8):
        a0, a1 = math.pi * i / 8, math.pi * (i + 1) / 8
        (x0, z0), (x1, z1) = ((0.2 * math.cos(a), 0.36 + 0.2 * math.sin(a)) for a in (a0, a1))
        k.box(((x0 + x1) / 2, 0.0, (z0 + z1) / 2), (math.hypot(x1 - x0, z1 - z0) + 0.01, 0.012, 0.012), zinc, rot=(0.0, -math.atan2(z1 - z0, x1 - x0), 0.0))
    return k.finish("bucket")


def candles():
    """Three candle stubs of different heights in a brass dish, wax run down their sides. About 0.24 x 0.17 x 0.24 m. The flames are drawn in code."""
    k = Kit()
    wax, brass = "candle_wax", "metal_plate_02"
    k.cone((0, 0, 0.012), 0.12, 0.12, 0.024, brass, segments=14)
    k.cone((0, 0, 0.028), 0.1, 0.1, 0.008, brass, segments=14)
    for (x, y, h, r) in ((-0.05, 0.03, 0.11, 0.022), (0.06, -0.02, 0.16, 0.02), (0.01, 0.07, 0.07, 0.025)):
        k.cone((x, y, 0.03 + h / 2), r, r * 0.9, h, wax, segments=8)
        k.box((x, y, 0.03 + h + 0.012), (0.004, 0.004, 0.024), brass)
        k.cone((x + r * 0.6, y, 0.03 + h * 0.6), 0.008, 0.005, h * 0.5, wax, segments=6)
    return k.finish("candles")


ARCHIVE_KITS = {
    "book_trolley": book_trolley,
    "book_pile": book_pile,
    "book_press": book_press,
    "guillotine": guillotine,
    "safe": safe,
    "deed_boxes": deed_boxes,
    "pump_engine": pump_engine,
    "bucket": bucket,
    "candles": candles,
    "card_catalogue": card_catalogue,
    "issue_desk": issue_desk,
    "reading_lamp": reading_lamp,
    "lectern": lectern,
    "theatre_bench": theatre_bench,
    "projector": projector,
    "display_case": display_case,
    "globe": globe,
}


SANATORIUM_KITS = {
    "gurney": gurney,
    "dressing_trolley": dressing_trolley,
    "locker_bank": locker_bank,
    "cot": cot,
    "mortuary_slab": mortuary_slab,
    "bath_tub": bath_tub,
    "rocking_horse": rocking_horse,
    "ambulance": ambulance,
}


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--src", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--only", choices=["crate", "sandbags", "headstone", *SANATORIUM_KITS, *ARCHIVE_KITS, *WHEEL_KITS, *VALVE_KITS, *(f"book_rows_{v}" for v in BOOK_VARIANTS)])
    parser.add_argument("--headstone", help="a high-poly headstone scan to decimate and bake")
    parser.add_argument("--work", help="where the headstone's baked PNGs go")
    args = parser.parse_args(argv)
    out = pathlib.Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    if args.only and args.only.startswith("book_rows_"):
        reset()
        obj = book_rows(args.only[len("book_rows_"):])
        export(obj, out, args.only, colours=True)
        book_rows_report(obj, args.only)
        return
    if args.only in WHEEL_KITS or args.only in VALVE_KITS:
        reset()
        obj = {**WHEEL_KITS, **VALVE_KITS}[args.only]()
        export(obj, out, args.only)
        wheel_report(obj, args.only)
        return
    if args.only in ARCHIVE_KITS:
        reset()
        obj = ARCHIVE_KITS[args.only]()
        export(obj, out, args.only)
        report(obj, args.only)
        return
    if args.only in SANATORIUM_KITS:
        reset()
        obj = SANATORIUM_KITS[args.only]()
        export(obj, out, args.only)
        report(obj, args.only)
        return
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
