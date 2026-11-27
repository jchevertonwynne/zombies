"""Run by Blender, headless, from `props.py`:

    blender --background --python tools/props/assemble.py -- <unit> <in.gltf> <out.gltf>

The Refinery's four units (#574) that are assembled from the pieces of a Poly Haven catalogue model, not the whole model decimated by
`decimate.py`: `modular_industrial_pipes_01` is a catalogue of separate pipe pieces (a long straight, a short one, bends, a tee, a cross and
a red-wheeled valve), `modular_fire_escape` a set of stairs, railings and a 6.5 m ladder, and `flood_lights` a set of lamp rigs. Each unit
takes the pieces it wants (decimated, in place, to a few hundred triangles), turns and places them, adds the few boxes that carry them
(saddles, legs, hoops) and stands the result as the fitting framework expects: its foot's middle at the origin, its length along +X, its depth
along +Z (glTF's Y is up). The pieces are not welded: they butt flange to flange, as the catalogue's do.

Units:  pipe_run (three long straights on two saddles), pipe_manifold (a straight, the valve, a tee and a straight on two legs, the tee's
branch up), fire_escape (a caged ladder: the model's ladder cut to 3 m and four hoops of boxes), flood_lights (a twin pair of the model's
wall-mounted heads on a cross bar and a bracket; it faces glTF +Z).

Each material is named for the catalogue group it samples (`g1`, `g2`): `props.py` gives them their KTX2 textures. Prints
`UNIT x y z tris=n`, the unit's size in the game's axes, and writes the glTF with its meshes and UVs only.
"""

import math
import sys

import bmesh
import bpy
from mathutils import Euler, Matrix, Vector

unit, src, dst = sys.argv[sys.argv.index("--") + 1:][:3]

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=src)
OBJECTS = {o.name: o for o in bpy.context.scene.objects if o.type == "MESH"}


def source(suffix):
    (name,) = [n for n in OBJECTS if n.endswith(suffix)]
    return OBJECTS[name]


def decimated(obj, target):
    """A copy of `obj`'s mesh (in world space) collapsed to about `target` triangles."""
    copy = obj.copy()
    copy.data = obj.data.copy()
    bpy.context.scene.collection.objects.link(copy)
    bpy.ops.object.select_all(action="DESELECT")
    copy.select_set(True)
    bpy.context.view_layer.objects.active = copy
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    tris = sum(len(p.vertices) - 2 for p in copy.data.polygons)
    if tris > target:
        mod = copy.modifiers.new("decimate", "DECIMATE")
        mod.decimate_type = "COLLAPSE"
        mod.ratio = target / tris
        mod.use_collapse_triangulate = True
        bpy.ops.object.modifier_apply(modifier=mod.name)
    return copy


class Assembly:
    """Pieces gathered into one mesh with a material slot per catalogue group."""

    def __init__(self, groups):
        self.bm = bmesh.new()
        self.groups = groups
        self.uv = self.bm.loops.layers.uv.new("UVMap")

    def piece(self, suffix, target, group, anchor, turn=0.0, at=(0.0, 0.0, 0.0), like=None):
        """A decimated piece: moved so its bound-box point `anchor` (0 to 1 on each axis, the piece's own, or `like`'s, the piece it is
        mounted on) is the origin, turned `turn` degrees in the x-z plane (the catalogue's plane), then set down at `at`."""
        obj = decimated(source(suffix), target)
        mesh = obj.data
        pts = [(source(like).matrix_world @ Vector(c)) for c in source(like).bound_box] if like else [v.co for v in mesh.vertices]
        lo = Vector(min(p[i] for p in pts) for i in range(3))
        hi = Vector(max(p[i] for p in pts) for i in range(3))
        pivot = Vector(lo[i] + (hi[i] - lo[i]) * anchor[i] for i in range(3))
        matrix = Matrix.Translation(Vector(at)) @ Euler((0.0, -math.radians(turn), 0.0), "XYZ").to_matrix().to_4x4() @ Matrix.Translation(-pivot)
        mesh.transform(matrix)
        self.add(mesh, group)
        bpy.data.objects.remove(obj)

    def add(self, mesh, group):
        before = {f.index for f in self.bm.faces}
        self.bm.from_mesh(mesh)
        self.bm.faces.ensure_lookup_table()
        for face in self.bm.faces:
            if face.index not in before:
                face.material_index = self.groups.index(group)

    def box(self, centre, size, group, rot=(0.0, 0.0, 0.0)):
        """A box of `size` (x, y, z), its UVs one corner of the catalogue's texture (the pieces' own are tiled by their meshes)."""
        matrix = Matrix.Translation(Vector(centre)) @ Euler(rot, "XYZ").to_matrix().to_4x4() @ Matrix.Diagonal(Vector((*size, 1.0)))
        made = bmesh.ops.create_cube(self.bm, size=1.0, matrix=matrix)
        for face in {f for v in made["verts"] for f in v.link_faces}:
            face.material_index = self.groups.index(group)
            for loop in face.loops:
                loop[self.uv].uv = (0.5, 0.5)

    def finish(self, name, size):
        """The mesh object, scaled per axis to `size` (length x, height, depth; a single number scales it all alike) and stood on its foot's
        middle."""
        bmesh.ops.triangulate(self.bm, faces=self.bm.faces[:])
        mesh = bpy.data.meshes.new(name)
        self.bm.to_mesh(mesh)
        self.bm.free()
        for group in self.groups:
            mat = bpy.data.materials.new(group)
            mat.use_nodes = True
            mesh.materials.append(mat)
        obj = bpy.data.objects.new(name, mesh)
        bpy.context.scene.collection.objects.link(obj)
        pts = [v.co for v in mesh.vertices]
        lo = Vector(min(p[i] for p in pts) for i in range(3))
        hi = Vector(max(p[i] for p in pts) for i in range(3))
        if isinstance(size, (int, float)):
            scale = Vector((size, size, size))
        else:
            want = Vector((size[0], size[2], size[1]))  # Blender's y is the depth, z the height
            scale = Vector(want[i] / (hi[i] - lo[i]) for i in range(3))
        mesh.transform(Matrix.Diagonal(Vector((*scale, 1.0))) @ Matrix.Translation(Vector(((-(lo.x + hi.x) / 2), (-(lo.y + hi.y) / 2), -lo.z))))
        return obj


# Straight pipes in the catalogue lie along the plane's z (up); `turn` 90 lays one along x. The long straight is `pipe02`, the short `pipe01`
# (group 1), the tee `pipe06`, the cross `pipe07`, the valve `pipe08` (group 2).
PIPE_DIAMETER = 0.208


def pipe_run():
    a = Assembly(["g1"])
    radius = PIPE_DIAMETER / 2
    rest = 0.25 + radius
    # Three long straights laid along x in a pyramid on two saddles.
    for dy, up in ((-radius - 0.006, rest), (radius + 0.006, rest), (0.0, rest + 0.18)):
        a.piece("pipe02", 650, "g1", (0.5, 0.5, 0.5), turn=90.0, at=(0.0, dy, up))
    for x in (-0.7, 0.7):
        a.box((x, 0.0, 0.125), (0.16, 2 * PIPE_DIAMETER + 0.04, 0.25), "g1")
    return a.finish("pipe_run", (2.3, 0.7, 0.5))


def pipe_manifold():
    a = Assembly(["g1", "g2"])
    height = 0.85
    # From the left along x: a short straight, the valve (its wheel facing -y), the tee (its run along x, its branch up) and a short straight.
    short, valve, tee = 0.86, 0.38, 0.36
    x = -(short + valve + tee + short) / 2
    a.piece("pipe01", 250, "g1", (0.5, 0.5, 0.5), turn=90.0, at=(x + short / 2, 0.0, height))
    x += short
    a.piece("pipe08", 700, "g2", (0.5, 0.5, 0.5), turn=90.0, at=(x + valve / 2, 0.0, height))
    x += valve
    # The tee's run axis is at x 0.4 of its box (its branch runs out the right side): anchor it by the run's middle.
    a.piece("pipe06", 420, "g2", (0.34, 0.5, 0.5), turn=90.0, at=(x + tee / 2, 0.0, height))
    x += tee
    a.piece("pipe01", 250, "g1", (0.5, 0.5, 0.5), turn=90.0, at=(x + short / 2, 0.0, height))
    for leg in (-0.95, 0.95):
        a.box((leg, 0.0, (height - 0.104) / 2), (0.12, 0.2, height - 0.104), "g1")
    a.box((0.0, 0.0, 0.03), (2.5, 0.4, 0.06), "g1")
    return a.finish("pipe_manifold", (2.5, 1.15, 0.4))


def fire_escape():
    a = Assembly(["g1"])
    # The ladder's lowest 3 m (the model's is 6.5 m tall: cut it between rungs), its feet on the floor.
    ladder = source("ladder_bottom")
    lo = min((ladder.matrix_world @ Vector(c)).z for c in ladder.bound_box)
    copy = decimated(ladder, 900)
    bm = bmesh.new()
    bm.from_mesh(copy.data)
    cut = lo + 3.0
    bmesh.ops.bisect_plane(bm, geom=bm.verts[:] + bm.edges[:] + bm.faces[:], plane_co=(0, 0, cut), plane_no=(0, 0, 1), clear_outer=True)
    bm.to_mesh(copy.data)
    bm.free()
    pts = [v.co for v in copy.data.vertices]
    cx = (min(p.x for p in pts) + max(p.x for p in pts)) / 2
    cy = (min(p.y for p in pts) + max(p.y for p in pts)) / 2
    copy.data.transform(Matrix.Translation(Vector((-cx, -cy + 0.28, -lo))))
    a.add(copy.data, "g1")
    bpy.data.objects.remove(copy)
    # The cage: four octagonal hoops of eight thin boxes round the ladder, joined by four uprights.
    radius, bar = 0.42, 0.025
    chord = 2 * radius * math.sin(math.pi / 8) + 0.01
    for up in (0.9, 1.5, 2.1, 2.7):
        for i in range(8):
            ang = i * math.tau / 8 + math.pi / 8
            a.box((radius * math.cos(ang), radius * math.sin(ang), up), (chord, bar, bar), "g1", rot=(0.0, 0.0, ang + math.pi / 2))
    for i in range(4):
        ang = math.pi / 8 + i * math.tau / 4
        a.box((radius * math.cos(ang), radius * math.sin(ang), 1.85), (bar, bar, 1.9), "g1")
    return a.finish("fire_escape", (0.86, 3.0, 0.86))


def flood_lights():
    a = Assembly(["g1"])
    # Two of the model's wall-mounted heads (the lamp and its base plate and bracket) side by side on a cross bar, the pair then scaled up
    # 2.4 times alike, for a mast 9 m up. The heads look out along -y here, which is the game's +z.
    for dx in (-0.2, 0.2):
        a.piece("wall_light", 750, "g1", (0.5, 0.5, 0.5), at=(dx, 0.0, 0.0), like="wall_light")
        a.piece("wall_base", 110, "g1", (0.5, 0.5, 0.5), at=(dx, 0.0, 0.0), like="wall_light")
    a.box((0.0, 0.1, 0.0), (0.7, 0.03, 0.05), "g1")
    return a.finish("flood_lights", 2.4)


UNITS = {"pipe_run": pipe_run, "pipe_manifold": pipe_manifold, "fire_escape": fire_escape, "flood_lights": flood_lights}
obj = UNITS[unit]()
bpy.ops.object.select_all(action="DESELECT")
obj.select_set(True)
bpy.context.view_layer.objects.active = obj
tris = len(obj.data.polygons)
assert tris <= 2100, f"{unit}: {tris} triangles"
bpy.ops.export_scene.gltf(
    filepath=dst,
    export_format="GLTF_SEPARATE",
    use_selection=True,
    export_image_format="NONE",
    export_materials="EXPORT",
    export_yup=True,
    export_cameras=False,
    export_lights=False,
)
pts = [v.co for v in obj.data.vertices]
size = [max(p[i] for p in pts) - min(p[i] for p in pts) for i in range(3)]
print("UNIT %.3f %.3f %.3f tris=%d" % (size[0], size[2], size[1], tris))
