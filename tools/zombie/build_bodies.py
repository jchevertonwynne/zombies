"""Builds a Zombie body: a MakeHuman body in official CC0 clothes, skinned to the Universal
Animation Library (UAL2) skeleton, as a glTF with no clips but the idle (the rest are in the clips
glTF `build_clips.py` makes, which fit any body because the rig is UAL2's own armature).

Run it in headless Blender with MPFB enabled (see tools/zombie/README.md, and `build.sh`, which
sets all of that up):

    blender -b --python tools/zombie/build_bodies.py -- --variant labourer \
        --ual2 UAL2_Standard.glb --clothes <dir holding clothes/> --out labourer.glb

What it makes
-------------
* A MakeHuman basemesh (MPFB), male, shaped by the variant (`labourer`, `soldier`, `civilian`),
  with its shirt and trousers of the official CC0 asset packs fitted to it.
* The body brought into the UAL rig's rest pose (a T pose) and skinned to that rig, with weights
  copied from UAL's own mannequin.
* The mesh split into parts the game can hide: `<Name>_Head`, `<Name>_LegL`, `<Name>_LegR` and
  their trouser legs, and the rest (`<Name>_Torso`, `<Name>_Shirt`, `<Name>_Pants`). The cuts the
  split leaves in the torso and the trousers (the neck and the thighs) are capped with faces whose
  UVs sit on a dark-red patch in the corner of each colour map, so a beheaded or gibbed body is
  closed, not hollow.

Textures are not in the glTF: the game dresses each part by name (Skin, Shirt, Pants) in KTX2 made
by `make_textures.py`.
"""

import math
import os
import sys

import bmesh
import bpy
from mathutils import Matrix, Vector

CLIPS = ("Zombie_Idle_Loop",)

# Where the cap faces' UVs go: the corner of every colour map, painted dark red by make_textures.py.
PATCH = 0.01

# Each body: its build (MakeHuman macros), and the shirt and trousers it wears (.mhclo under clothes/).
VARIANTS = {
    # A gaunt, tall labourer in a fisherman sweater and wool trousers.
    "labourer": dict(
        name="Labourer",
        macro={"gender": 1.0, "age": 0.62, "muscle": 0.4, "weight": 0.3, "proportions": 0.5, "height": 0.55},
        shirt="toigo_fisherman_sweater/toigo_fisherman_sweater.mhclo",
        pants="toigo_wool_pants/toigo_wool_pants.mhclo",
    ),
    # A broad soldier: a tucked tee and cargo trousers.
    "soldier": dict(
        name="Soldier",
        macro={"gender": 1.0, "age": 0.5, "muscle": 0.8, "weight": 0.55, "proportions": 0.55, "height": 0.62},
        shirt="toigo_basic_tucked_t-shirt/toigo_basic_tucked_t-shirt.mhclo",
        pants="cortu_cargo_pants/cortu_cargo_pants.mhclo",
    ),
    # A heavy-set civilian: a polo shirt and jean shorts.
    "civilian": dict(
        name="Civilian",
        macro={"gender": 1.0, "age": 0.72, "muscle": 0.3, "weight": 0.8, "proportions": 0.45, "height": 0.42},
        shirt="namuhekam_male_polo_shirt/namuhekam_male_polo_shirt.mhclo",
        pants="cortu_jeans_shorts/cortu_jeans_shorts.mhclo",
    ),
    # The specials (see GEAR): each its own build, in clothes whose maps a plain body already uses.
    # A bulky Napalm in a burnt polo and shorts, under its tank.
    "napalm": dict(
        name="Napalm",
        macro={"gender": 1.0, "age": 0.6, "muscle": 0.5, "weight": 0.9, "proportions": 0.5, "height": 0.5},
        shirt="namuhekam_male_polo_shirt/namuhekam_male_polo_shirt.mhclo",
        pants="cortu_jeans_shorts/cortu_jeans_shorts.mhclo",
        bulk=(1.2, 1.15),
    ),
    # A starved, tall Shrieker with arms too long for it, in the labourer's sweater.
    "shrieker": dict(
        name="Shrieker",
        macro={"gender": 1.0, "age": 0.85, "muscle": 0.0, "weight": 0.0, "proportions": 0.5, "height": 0.72},
        shirt="toigo_fisherman_sweater/toigo_fisherman_sweater.mhclo",
        pants="toigo_wool_pants/toigo_wool_pants.mhclo",
        arm_stretch=1.4,
    ),
    # A huge Sapper in armour: a helmet, a chest and back plate and pauldrons.
    "sapper": dict(
        name="Sapper",
        macro={"gender": 1.0, "age": 0.5, "muscle": 1.0, "weight": 0.95, "proportions": 0.55, "height": 0.8},
        shirt="toigo_basic_tucked_t-shirt/toigo_basic_tucked_t-shirt.mhclo",
        pants="cortu_cargo_pants/cortu_cargo_pants.mhclo",
        bulk=(1.3, 1.25),
    ),
    # A Nova Crawler: a wiry body in a gas mask with canisters on its back, long arms for clawing.
    "nova": dict(
        name="Nova",
        macro={"gender": 1.0, "age": 0.45, "muscle": 0.3, "weight": 0.15, "proportions": 0.5, "height": 0.45},
        shirt="toigo_basic_tucked_t-shirt/toigo_basic_tucked_t-shirt.mhclo",
        pants="cortu_cargo_pants/cortu_cargo_pants.mhclo",
        arm_stretch=1.25,
    ),
}


def args():
    argv = sys.argv[sys.argv.index("--") + 1 :]
    out = {}
    for i in range(0, len(argv), 2):
        out[argv[i].lstrip("-")] = argv[i + 1]
    return out


def select_only(obj):
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj


def world_bounds(obj):
    pts = [obj.matrix_world @ v.co for v in obj.data.vertices]
    lo = Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
    hi = Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
    return lo, hi


def apply_transform(obj):
    select_only(obj)
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)


def pose_arms_down(arm, angle):
    """Turns each upper arm down about its shoulder by `angle`, the left one down and the right
    one down, in world space."""
    bpy.context.view_layer.update()
    for side, sign in (("l", 1.0), ("r", -1.0)):
        bone = arm.pose.bones["upperarm_" + side]
        head = bone.matrix.translation.copy()
        turn = Matrix.Translation(head) @ Matrix.Rotation(sign * angle, 4, "Y") @ Matrix.Translation(-head)
        bone.matrix = turn @ bone.matrix
        bpy.context.view_layer.update()


def dominant_bone(obj, vertex):
    best, name = 0.0, None
    for g in vertex.groups:
        if g.weight > best:
            best, name = g.weight, obj.vertex_groups[g.group].name
    return name


def split_off(obj, new_name, wanted):
    """Moves the faces `wanted(face)` into a new object `new_name` and returns it (or None)."""
    mesh = obj.data
    chosen_set = {p.index for p in mesh.polygons if wanted(p)}
    if not chosen_set:
        return None
    select_only(obj)
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_mode(type="FACE")
    bpy.ops.mesh.select_all(action="DESELECT")
    bm = bmesh.from_edit_mesh(obj.data)
    bm.faces.ensure_lookup_table()
    for f in bm.faces:
        f.select_set(f.index in chosen_set)
    bmesh.update_edit_mesh(obj.data)
    bpy.ops.mesh.separate(type="SELECTED")
    bpy.ops.object.mode_set(mode="OBJECT")
    made = [o for o in bpy.context.selected_objects if o != obj]
    new = made[0]
    new.name = new_name
    new.data.name = new_name
    return new


def parts_of(obj, prefix, classify):
    """Splits `obj` into pieces by `classify(face_bones) -> suffix`; the leftover keeps `obj`."""
    # Each face's class, from the dominant bone of each of its corners.
    mesh = obj.data
    vertex_bone = [dominant_bone(obj, v) for v in mesh.vertices]
    face_class = {}
    for poly in mesh.polygons:
        face_class[poly.index] = classify([vertex_bone[i] for i in poly.vertices])
    made = {}
    for suffix in sorted({c for c in face_class.values() if c}):
        piece = split_off(obj, prefix + suffix, lambda f, suffix=suffix, fc=face_class: fc[f.index] == suffix)
        made[suffix] = piece
        # Faces were renumbered by the separation: recompute for the remainder.
        mesh = obj.data
        vertex_bone = [dominant_bone(obj, v) for v in mesh.vertices]
        face_class = {p.index: classify([vertex_bone[i] for i in p.vertices]) for p in mesh.polygons}
    return made


LEG_L = {"thigh_l", "calf_l", "foot_l", "ball_l", "ball_leaf_l"}
LEG_R = {"thigh_r", "calf_r", "foot_r", "ball_r", "ball_leaf_r"}


def leg_class(bones):
    if all(b in LEG_L for b in bones):
        return "LegL"
    if all(b in LEG_R for b in bones):
        return "LegR"
    return None


def skin_class(bones):
    head = sum(1 for b in bones if b == "Head")
    if head == len(bones):
        return "Head"
    return leg_class(bones)


def key_of(vertex):
    """A vertex's place, rounded, so a vertex split from its twin is still found."""
    return tuple(round(c, 4) for c in vertex.co)


def boundary_keys(obj):
    """The edges of `obj` that lie on its open boundary, as pairs of places."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    edges = set()
    for edge in bm.edges:
        if len(edge.link_faces) == 1:
            edges.add(tuple(sorted((key_of(edge.verts[0]), key_of(edge.verts[1])))))
    bm.free()
    return edges


def cap_cuts(obj, natural):
    """Closes the cuts a split left in `obj`: the boundary edges it didn't have before it was split
    (the neck of the torso, the thighs of the trousers). Each loop becomes triangles whose UVs sit on
    the dark-red patch in the colour maps' corner."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    uv = bm.loops.layers.uv.verify()
    cuts = []
    for edge in bm.edges:
        if len(edge.link_faces) != 1:
            continue
        pair = tuple(sorted((key_of(edge.verts[0]), key_of(edge.verts[1]))))
        if pair not in natural:
            cuts.append(edge)
    if cuts:
        before = set(bm.faces)
        bmesh.ops.edgeloop_fill(bm, edges=cuts)
        made = [f for f in bm.faces if f not in before]
        tri = bmesh.ops.triangulate(bm, faces=made, quad_method="BEAUTY", ngon_method="BEAUTY")["faces"]
        centre = sum((v.co for v in bm.verts), Vector()) / len(bm.verts)
        for face in tri:
            if face.normal.dot(face.calc_center_median() - centre) < 0:
                face.normal_flip()
            for loop in face.loops:
                loop[uv].uv = (PATCH, PATCH)
    print("CAPPED", obj.name, len(cuts), "cut edges")
    bm.to_mesh(obj.data)
    bm.free()


def flatten_caps(obj):
    """Shades the cap faces flat, so the red patch doesn't smear onto the body around them."""
    mesh = obj.data
    if not mesh.uv_layers:
        return
    layer = mesh.uv_layers.active.data
    for poly in mesh.polygons:
        if all(layer[i].uv[0] < PATCH * 2 and layer[i].uv[1] < PATCH * 2 for i in poly.loop_indices):
            poly.use_smooth = False


def make_material(name, rgb, roughness=0.9):
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    node = mat.node_tree.nodes["Principled BSDF"]
    node.inputs["Base Color"].default_value = (*rgb, 1.0)
    node.inputs["Roughness"].default_value = roughness
    return mat


# ---- Gear: the parts that give each special its own silhouette ----

# The name of each gear part (after the body's prefix), and whether it glows (ZombieGlow) rather than
# being plain gear (ZombieGear). A part's name decides what the game does with it: a name ending in
# `Head` goes with the head, and `Helmet` is the Sapper's, which comes off.
GEAR_PARTS = {
    "Tank": False,
    "Flame": True,
    "Helmet": False,
    "Plate": False,
    "PauldronL": False,
    "PauldronR": False,
    "MaskHead": False,
    "LensHead": True,
    "Canister": True,
}


def bone_pos(arm, name, tail=False):
    b = arm.data.bones[name]
    return arm.matrix_world @ (b.tail_local if tail else b.head_local)


def gear_object(arm, name, bone, build):
    """A mesh `name` made by `build(bmesh)` (in world space), bound wholly to `bone`."""
    mesh = bpy.data.meshes.new(name)
    bm = bmesh.new()
    build(bm)
    bm.to_mesh(mesh)
    bm.free()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    group = obj.vertex_groups.new(name=bone)
    group.add(list(range(len(mesh.vertices))), 1.0, "REPLACE")
    obj.parent = arm
    mod = obj.modifiers.new("Armature", "ARMATURE")
    mod.object = arm
    for poly in mesh.polygons:
        poly.use_smooth = True
    mesh.uv_layers.new(name="UVMap")
    return obj


def add_cylinder(bm, centre, radius, depth, top=None, segments=14):
    """A cylinder standing on Z (a cone to `top` radius when given)."""
    bmesh.ops.create_cone(
        bm,
        cap_ends=True,
        cap_tris=False,
        segments=segments,
        radius1=radius,
        radius2=radius if top is None else top,
        depth=depth,
        matrix=Matrix.Translation(centre),
    )


def add_box(bm, centre, size):
    m = Matrix.Translation(centre) @ Matrix.Diagonal(Vector((size[0], size[1], size[2], 1.0)))
    bmesh.ops.create_cube(bm, size=1.0, matrix=m)


def add_dome(bm, centre, radius, squash=1.0, cut=0.0):
    """A sphere whose part below `centre.z + cut` is cut away: a dome."""
    m = Matrix.Translation(centre) @ Matrix.Diagonal(Vector((1.0, 1.0, squash, 1.0)))
    made = bmesh.ops.create_uvsphere(bm, u_segments=16, v_segments=10, radius=radius, matrix=m)["verts"]
    doomed = [v for v in made if v.co.z < centre.z + cut - 1e-4]
    bmesh.ops.delete(bm, geom=doomed, context="VERTS")


def torso_back(prefix, z_lo, z_hi, sign):
    """The Y of the torso's back (sign 1) or front (-1) between two heights, for putting gear on it."""
    torso = bpy.data.objects[prefix + "Torso"]
    ys = [v.co.y for v in torso.data.vertices if z_lo <= v.co.z <= z_hi and abs(v.co.x) < 0.2]
    return max(ys) if sign > 0 else min(ys)


def head_box(prefix):
    lo, hi = world_bounds(bpy.data.objects[prefix + "Head"])
    return (lo + hi) / 2, (hi.z - lo.z) / 2, lo, hi


def napalm_gear(arm, prefix):
    chest = bone_pos(arm, "spine_02")
    top = bone_pos(arm, "spine_03", tail=True)
    back = torso_back(prefix, chest.z, top.z, 1)
    for x in (-0.11, 0.11):
        centre = Vector((x, back + 0.08, (chest.z + top.z) / 2 + 0.02))

        def tank(bm, centre=centre):
            add_cylinder(bm, centre, 0.1, 0.5)
            add_dome(bm, centre + Vector((0, 0, 0.25)), 0.1, 0.8)

        gear_object(arm, prefix + "Tank" + ("" if x < 0 else ".001"), "spine_03", tank)
    flame_at = Vector((-0.11, back + 0.08, (chest.z + top.z) / 2 + 0.5))
    gear_object(arm, prefix + "Flame", "spine_03", lambda bm: add_cylinder(bm, flame_at, 0.08, 0.34, top=0.0, segments=10))


def shrieker_gear(arm, prefix):
    pass


def sapper_gear(arm, prefix):
    chest = bone_pos(arm, "spine_02")
    top = bone_pos(arm, "spine_03", tail=True)
    front = torso_back(prefix, chest.z, top.z, -1)
    back = torso_back(prefix, chest.z, top.z, 1)
    mid = (chest.z + top.z) / 2

    def plate(bm):
        add_box(bm, Vector((0, front - 0.03, mid)), (0.4, 0.1, 0.4))
        add_box(bm, Vector((0, back + 0.03, mid)), (0.4, 0.1, 0.44))

    gear_object(arm, prefix + "Plate", "spine_03", plate)
    for name, bone, sign in (("PauldronL", "upperarm_l", 1), ("PauldronR", "upperarm_r", -1)):
        at = bone_pos(arm, bone)
        centre = Vector((at.x + sign * 0.03, at.y, at.z + 0.04))
        gear_object(arm, prefix + name, bone, lambda bm, c=centre: add_dome(bm, c, 0.17, 0.7, cut=-0.05))
    centre, radius, lo, hi = head_box(prefix)
    dome_at = Vector((centre.x, centre.y, centre.z + radius * 0.1))

    def helmet(bm):
        add_dome(bm, dome_at, radius * 1.18, 1.0, cut=-radius * 0.15)
        add_cylinder(bm, Vector((centre.x, centre.y, dome_at.z - radius * 0.15)), radius * 1.3, 0.03)

    gear_object(arm, prefix + "Helmet", "Head", helmet)


def nova_gear(arm, prefix):
    centre, radius, lo, hi = head_box(prefix)
    face = lo.y  # the face looks along -Y
    mask_at = Vector((centre.x, face + 0.01, centre.z - radius * 0.35))

    def mask(bm):
        # A snout, and a strap round the head.
        bmesh.ops.create_cone(
            bm, cap_ends=True, segments=12, radius1=radius * 0.55, radius2=radius * 0.3, depth=radius * 0.9,
            matrix=Matrix.Translation(mask_at + Vector((0, -radius * 0.25, 0))) @ Matrix.Rotation(math.pi / 2, 4, "X"),
        )
        add_cylinder(bm, Vector((centre.x, centre.y, centre.z + radius * 0.1)), radius * 0.86, 0.05)

    gear_object(arm, prefix + "MaskHead", "Head", mask)

    def lens(bm):
        for x in (-0.45, 0.45):
            bmesh.ops.create_uvsphere(
                bm, u_segments=8, v_segments=6, radius=radius * 0.22,
                matrix=Matrix.Translation(Vector((centre.x + x * radius, face + radius * 0.1, centre.z + radius * 0.2))),
            )

    gear_object(arm, prefix + "LensHead", "Head", lens)
    chest = bone_pos(arm, "spine_02")
    top = bone_pos(arm, "spine_03", tail=True)
    back = torso_back(prefix, chest.z, top.z, 1)

    def canisters(bm):
        for x in (-0.09, 0.09):
            add_cylinder(bm, Vector((x, back + 0.1, (chest.z + top.z) / 2)), 0.07, 0.4)

    gear_object(arm, prefix + "Canister", "spine_03", canisters)


GEAR = {
    "labourer": lambda arm, prefix: None,
    "soldier": lambda arm, prefix: None,
    "civilian": lambda arm, prefix: None,
    "napalm": napalm_gear,
    "shrieker": shrieker_gear,
    "sapper": sapper_gear,
    "nova": nova_gear,
}


def main():
    a = args()
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete()

    # The UAL2 rig, its mannequin (the weight source) and its clips.
    bpy.ops.import_scene.gltf(filepath=a["ual2"])
    arm = next(o for o in bpy.data.objects if o.type == "ARMATURE")
    mannequin = next(o for o in bpy.data.objects if o.name == "Mannequin")
    for o in list(bpy.data.objects):
        if o not in (arm, mannequin):
            bpy.data.objects.remove(o)
    for action in list(bpy.data.actions):
        if action.name not in CLIPS:
            bpy.data.actions.remove(action)
    for track in arm.animation_data.nla_tracks:
        arm.animation_data.nla_tracks.remove(track)
    arm.animation_data.action = bpy.data.actions["Zombie_Idle_Loop"]
    for clip in CLIPS:
        bpy.data.actions[clip].use_fake_user = True
    arm.animation_data.action = None
    for pb in arm.pose.bones:
        pb.location = (0, 0, 0)
        pb.rotation_quaternion = (1, 0, 0, 0)
        pb.rotation_euler = (0, 0, 0)
        pb.scale = (1, 1, 1)
    bpy.context.view_layer.update()

    # A MakeHuman body, a gaunt man, in the game's scale.
    from bl_ext.user_default.mpfb.services.humanservice import HumanService
    from bl_ext.user_default.mpfb.services.targetservice import TargetService

    variant = VARIANTS[a["variant"]]
    prefix = variant["name"] + "_"
    macro = TargetService.get_default_macro_info_dict()
    macro.update(variant["macro"])
    human = HumanService.create_human(
        mask_helpers=False, detailed_helpers=False, extra_vertex_groups=False, feet_on_ground=True, scale=0.1, macro_detail_dict=macro
    )
    human.name = prefix + "Torso"
    select_only(human)
    bpy.ops.object.shape_key_remove(all=True, apply_mix=True)
    clothes = []
    for rel in (variant["shirt"], variant["pants"]):
        path = os.path.join(a["clothes"], "clothes", rel)
        item = HumanService.add_mhclo_asset(
            path,
            human,
            asset_type="Clothes",
            subdiv_levels=0,
            material_type="GAMEENGINE",
            set_up_rigging=False,
            interpolate_weights=False,
            import_subrig=False,
            import_weights=False,
        )
        clothes.append(item)
    shirt, pants = clothes
    shirt.name = prefix + "Shirt"
    pants.name = prefix + "Pants"

    # Drop the basemesh's helper geometry: the skin is the `body` group.
    select_only(human)
    bm = bmesh.new()
    bm.from_mesh(human.data)
    deform = bm.verts.layers.deform.verify()
    body_index = human.vertex_groups["body"].index
    doomed = [v for v in bm.verts if body_index not in v[deform]]
    bmesh.ops.delete(bm, geom=doomed, context="VERTS")
    bm.to_mesh(human.data)
    bm.free()

    parts = [human, shirt, pants]
    for part in parts:
        part.parent = None
        part.vertex_groups.clear()
        for m in list(part.modifiers):
            part.modifiers.remove(m)
        part.data.materials.clear()
        apply_transform(part)

    # Fit it to the mannequin: the same height, standing in the same place.
    m_lo, m_hi = world_bounds(mannequin)
    h_lo, h_hi = world_bounds(human)
    scale = (m_hi.z - m_lo.z) / (h_hi.z - h_lo.z)
    for part in parts:
        part.scale = (scale, scale, scale)
        apply_transform(part)
    h_lo, h_hi = world_bounds(human)
    shift = Vector(((m_lo.x + m_hi.x) / 2 - (h_lo.x + h_hi.x) / 2, (m_lo.y + m_hi.y) / 2 - (h_lo.y + h_hi.y) / 2, m_lo.z - h_lo.z))
    for part in parts:
        part.location = shift
        apply_transform(part)

    # Stand the clothes a little off the skin, so the body doesn't show through them.
    for item in (shirt, pants):
        select_only(item)
        mod = item.modifiers.new("lift", "DISPLACE")
        mod.direction = "NORMAL"
        mod.strength = 0.012
        mod.mid_level = 0.0
        bpy.ops.object.modifier_apply(modifier=mod.name)

    # MakeHuman stands with its arms angled down; UAL stands in a T. Pose the mannequin to the
    # body's arm angle, copy its weights over, then bring the body up to the T.
    h_lo, h_hi = world_bounds(human)
    shoulder = arm.pose.bones["upperarm_l"].head.x
    reach = max(v.co.x for v in mannequin.data.vertices) - shoulder
    angle = math.acos(max(-1.0, min(1.0, (h_hi.x - shoulder) / reach)))
    print("ARM ANGLE", math.degrees(angle), "SCALE", scale)

    pose_arms_down(arm, angle)
    posed = mannequin.copy()
    posed.data = mannequin.data.copy()
    bpy.context.collection.objects.link(posed)
    select_only(posed)
    for m in posed.modifiers:
        if m.type == "ARMATURE":
            bpy.ops.object.modifier_apply(modifier=m.name)
    for part in parts:
        select_only(part)
        for g in posed.vertex_groups:
            part.vertex_groups.new(name=g.name)
        mod = part.modifiers.new("transfer", "DATA_TRANSFER")
        mod.object = posed
        mod.use_vert_data = True
        mod.data_types_verts = {"VGROUP_WEIGHTS"}
        mod.vert_mapping = "POLYINTERP_NEAREST"
        bpy.ops.object.datalayout_transfer(modifier=mod.name)
        bpy.ops.object.modifier_apply(modifier=mod.name)

    # A copy of the rig whose rest pose is the bent one, to carry the body back to the T.
    rest_matrices = {}
    bpy.context.view_layer.update()
    for pb in arm.pose.bones:
        rest_matrices[pb.name] = pb.matrix.copy()
    # Rest (T) matrices: from the bones themselves.
    t_matrices = {b.name: b.matrix_local.copy() for b in arm.data.bones}
    bent = arm.copy()
    bent.data = arm.data.copy()
    bpy.context.collection.objects.link(bent)
    select_only(bent)
    bpy.ops.object.mode_set(mode="POSE")
    bpy.ops.pose.select_all(action="SELECT")
    bpy.ops.pose.armature_apply(selected=False)
    bpy.ops.object.mode_set(mode="OBJECT")
    for part in parts:
        mod = part.modifiers.new("bend", "ARMATURE")
        mod.object = bent
    for pb in bent.pose.bones:
        pb.rotation_mode = "QUATERNION"
    bpy.context.view_layer.update()
    ordered = sorted(bent.pose.bones, key=lambda b: len(b.parent_recursive))
    for pb in ordered:
        pb.matrix = t_matrices[pb.name]
        bpy.context.view_layer.update()
    for part in parts:
        select_only(part)
        bpy.ops.object.modifier_apply(modifier="bend")

    # Arms too long for the skeleton (a Shrieker's, a Nova Crawler's): the mesh is stretched out from
    # the shoulder, its hands carried along rather than stretched. It still follows the same bones.
    stretch = variant.get("arm_stretch")
    if stretch:
        shoulder_x = arm.data.bones["upperarm_l"].head_local.x
        wrist_x = arm.data.bones["hand_l"].head_local.x - shoulder_x
        for part in parts:
            for v in part.data.vertices:
                side = 1.0 if v.co.x > 0 else -1.0
                d = abs(v.co.x) - shoulder_x
                if d <= 0:
                    continue
                v.co.x = side * (shoulder_x + (d * stretch if d < wrist_x else wrist_x * stretch + (d - wrist_x)))

    # Bulk (a Napalm's, the Sapper's): the trunk and legs widened about the body's middle, by how much
    # each vertex follows the spine, pelvis, thighs and calves, so the arms, hands and head keep their size.
    bulk = variant.get("bulk")
    if bulk:
        girth_bones = ("spine", "pelvis", "thigh", "calf", "clavicle")
        centre_y = sum(v.co.y for v in human.data.vertices) / len(human.data.vertices)
        for part in parts:
            names = {g.index: g.name for g in part.vertex_groups}
            for v in part.data.vertices:
                w = sum(g.weight for g in v.groups if names[g.group].startswith(girth_bones))
                k = min(1.0, w)
                v.co.x *= 1.0 + (bulk[0] - 1.0) * k
                v.co.y = centre_y + (v.co.y - centre_y) * (1.0 + (bulk[1] - 1.0) * k)

    # Bind the T-posed body to the real rig.
    for part in parts:
        part.parent = arm
        mod = part.modifiers.new("Armature", "ARMATURE")
        mod.object = arm
    bpy.data.objects.remove(bent)
    bpy.data.objects.remove(posed)
    bpy.data.objects.remove(mannequin)
    pose_arms_down(arm, 0.0)
    for pb in arm.pose.bones:
        pb.matrix_basis = Matrix()

    # The parts the game can hide, and the cut faces they leave capped.
    natural = {id(human): boundary_keys(human), id(pants): boundary_keys(pants)}
    parts_of(human, prefix, skin_class)
    parts_of(pants, prefix + "Pants", leg_class)
    cap_cuts(human, natural[id(human)])
    cap_cuts(pants, natural[id(pants)])

    # A special's gear: separate parts, each bound rigidly to a bone.
    GEAR[a["variant"]](arm, prefix)

    # Materials, named for what the game dresses them in.
    skin = make_material("ZombieSkin", (0.55, 0.62, 0.5))
    shirt_m = make_material("ZombieShirt", (0.3, 0.3, 0.28))
    pants_m = make_material("ZombiePants", (0.2, 0.2, 0.22))
    gear_m = make_material("ZombieGear", (0.3, 0.3, 0.3))
    glow_m = make_material("ZombieGlow", (1.0, 0.5, 0.1))
    for obj in bpy.data.objects:
        if obj.type != "MESH":
            continue
        obj.data.materials.clear()
        kind = obj.name.split("_", 1)[-1].split(".")[0]
        if kind in GEAR_PARTS:
            obj.data.materials.append(glow_m if GEAR_PARTS[kind] else gear_m)
        else:
            obj.data.materials.append(skin if "Shirt" not in obj.name and "Pants" not in obj.name else (shirt_m if "Shirt" in obj.name else pants_m))
        select_only(obj)
        bpy.ops.object.shade_smooth()
        flatten_caps(obj)

    for obj in bpy.data.objects:
        print("PART", obj.name, obj.type, len(obj.data.vertices) if obj.type == "MESH" else "")

    arm.animation_data_create()
    arm.animation_data.action = bpy.data.actions["Zombie_Idle_Loop"]
    out = a["out"]
    os.makedirs(os.path.dirname(out), exist_ok=True)
    bpy.ops.export_scene.gltf(
        filepath=out,
        export_format="GLB",
        export_yup=True,
        export_apply=False,
        export_skins=True,
        export_animations=True,
        export_animation_mode="ACTIONS",
        export_force_sampling=True,
        export_optimize_animation_size=True,
        export_image_format="NONE",
        export_cameras=False,
        export_lights=False,
    )
    print("WROTE", out, os.path.getsize(out))


main()
