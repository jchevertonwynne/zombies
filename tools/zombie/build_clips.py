"""Builds the clips every skinned Zombie plays, as one glTF with just the UAL2 skeleton (no mesh):

* from UAL2 (the rig's own clips, no retargeting): `Zombie_Idle_Loop`, `Zombie_Walk_Fwd_Loop`,
  `Zombie_Scratch`;
* from the UAL1 OpenGameArt mirror (a different skeleton, `DEF-*` bones), retargeted onto UAL2's by
  world-space rotation: `Jog_Fwd_Loop`, `Sprint_Loop`, `Death01`, `Hit_Chest`.

It also prints how fast the ground moves under the feet in each locomotion clip (`FOOT SPEED`), which
the game's playback speeds come from.

    blender -b --python tools/walker/build_clips.py -- --ual1 AnimationLibrary_Godot_Standard.glb \
        --ual2 UAL2_Standard.glb --out clips.glb
"""

import os
import statistics
import sys

import bpy
from mathutils import Matrix

UAL2_CLIPS = ("Zombie_Idle_Loop", "Zombie_Walk_Fwd_Loop", "Zombie_Scratch")
UAL1_CLIPS = ("Jog_Fwd_Loop", "Sprint_Loop", "Death01", "Hit_Chest")

# UAL1 bone -> UAL2 bone. Fingers are left at rest.
BONES = {
    "DEF-hips": "pelvis", "DEF-spine.001": "spine_01", "DEF-spine.002": "spine_02", "DEF-spine.003": "spine_03",
    "DEF-neck": "neck_01", "DEF-head": "Head",
}
for side, s in (("L", "l"), ("R", "r")):
    BONES.update({
        f"DEF-shoulder.{side}": f"clavicle_{s}", f"DEF-upper_arm.{side}": f"upperarm_{s}",
        f"DEF-forearm.{side}": f"lowerarm_{s}", f"DEF-hand.{side}": f"hand_{s}",
        f"DEF-thigh.{side}": f"thigh_{s}", f"DEF-shin.{side}": f"calf_{s}",
        f"DEF-foot.{side}": f"foot_{s}", f"DEF-toe.{side}": f"ball_{s}",
    })
TARGET_TO_SOURCE = {v: k for k, v in BONES.items()}


def args():
    argv = sys.argv[sys.argv.index("--") + 1 :]
    return {argv[i].lstrip("-"): argv[i + 1] for i in range(0, len(argv), 2)}


def armature_of(objs):
    return next(o for o in objs if o.type == "ARMATURE")


def retarget(src, tgt, action, name):
    """Bakes `action` (on `src`) onto `tgt` as a new action `name`."""
    scene = bpy.context.scene
    src.animation_data_create()
    src.animation_data.action = action
    tgt.animation_data_create()
    new = bpy.data.actions.new(name)
    tgt.animation_data.action = new
    order = sorted((pb for pb in tgt.pose.bones if pb.name in TARGET_TO_SOURCE), key=lambda b: len(b.parent_recursive))
    height = tgt.data.bones["pelvis"].head_local.z / src.data.bones["DEF-hips"].head_local.z
    first, last = int(action.frame_range[0]), int(action.frame_range[1])
    for frame in range(first, last + 1):
        scene.frame_set(frame)
        for pb in tgt.pose.bones:
            pb.matrix_basis = Matrix()
        bpy.context.view_layer.update()
        for pb in order:
            sb = src.pose.bones[TARGET_TO_SOURCE[pb.name]]
            s = src.matrix_world @ sb.matrix
            s0 = src.matrix_world @ src.data.bones[sb.name].matrix_local
            delta = s.to_3x3() @ s0.to_3x3().inverted()
            t0 = tgt.matrix_world @ tgt.data.bones[pb.name].matrix_local
            rot = (delta @ t0.to_3x3()).to_4x4()
            if pb.name == "pelvis":
                at = t0.translation + (s.translation - s0.translation) * height
            else:
                at = (tgt.matrix_world @ pb.matrix).translation
            world = Matrix.Translation(at) @ rot
            pb.matrix = tgt.matrix_world.inverted() @ world
            bpy.context.view_layer.update()
        for pb in order:
            pb.keyframe_insert("rotation_quaternion", frame=frame)
            if pb.name == "pelvis":
                pb.keyframe_insert("location", frame=frame)
    tgt.animation_data.action = None
    new.use_fake_user = True
    return new


def foot_speed(arm, action):
    """Metres a second the ground slides back under the planted feet while `action` plays."""
    scene = bpy.context.scene
    arm.animation_data_create()
    arm.animation_data.action = action
    first, last = int(action.frame_range[0]), int(action.frame_range[1])
    fps = scene.render.fps
    speeds = []
    for side in ("foot_l", "foot_r"):
        series = []
        for frame in range(first, last + 1):
            scene.frame_set(frame)
            bpy.context.view_layer.update()
            p = (arm.matrix_world @ arm.pose.bones[side].matrix).translation
            series.append((frame, p.y, p.z))
        ys = [y for _, y, _ in series]
        print("FOOT RANGE", action.name, side, round(max(ys) - min(ys), 3), "m over", len(series) - 1, "frames at", fps, "fps")
        low = min(z for _, _, z in series)
        planted = [(f, y) for f, y, z in series if z < low + 0.04]
        runs, run = [], []
        for item in planted:
            if run and item[0] != run[-1][0] + 1:
                runs.append(run)
                run = []
            run.append(item)
        if run:
            runs.append(run)
        for run in runs:
            if len(run) >= 3:
                speeds.append(abs((run[-1][1] - run[0][1]) / ((run[-1][0] - run[0][0]) / fps)))
    arm.animation_data.action = None
    return statistics.median(speeds) if speeds else 0.0


def main():
    a = args()
    bpy.ops.import_scene.gltf(filepath=a["ual2"])
    tgt = armature_of(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=a["ual1"])
    src = armature_of([o for o in bpy.data.objects if o != tgt])
    keep2 = {n: bpy.data.actions[n] for n in UAL2_CLIPS}
    for obj in list(bpy.data.objects):
        if obj not in (src, tgt):
            bpy.data.objects.remove(obj)
    for arm in (src, tgt):
        if arm.animation_data:
            for track in list(arm.animation_data.nla_tracks):
                arm.animation_data.nla_tracks.remove(track)
        for pb in arm.pose.bones:
            pb.rotation_mode = "QUATERNION"

    made = {}
    for clip in UAL1_CLIPS:
        made[clip] = retarget(src, tgt, bpy.data.actions[clip], "R_" + clip)
    for name, action in keep2.items():
        action.use_fake_user = True
        made[name] = action
    for action in list(bpy.data.actions):
        if action not in made.values():
            bpy.data.actions.remove(action)
    for clip in UAL1_CLIPS:
        made[clip].name = clip
    for name in ("Zombie_Walk_Fwd_Loop", "Jog_Fwd_Loop", "Sprint_Loop"):
        print("FOOT SPEED", name, round(foot_speed(tgt, made[name]), 3))
    for name, action in made.items():
        print("CLIP", name, action.name, [round(x) for x in action.frame_range])
    bpy.data.objects.remove(src)
    tgt.animation_data.action = made["Zombie_Idle_Loop"]
    os.makedirs(os.path.dirname(a["out"]), exist_ok=True)
    bpy.ops.export_scene.gltf(
        filepath=a["out"], export_format="GLB", export_yup=True, export_skins=True, export_animations=True,
        export_animation_mode="ACTIONS", export_force_sampling=True, export_optimize_animation_size=True,
        export_image_format="NONE", export_cameras=False, export_lights=False,
    )
    print("WROTE", a["out"], os.path.getsize(a["out"]))


main()
