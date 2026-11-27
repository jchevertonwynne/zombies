#!/usr/bin/env python3
"""Fetch Poly Haven models at 1K and turn each into a unit model the game fits into its Props:
decimated by Blender (headless, `decimate.py`) to at most ~2k triangles, its textures packed into KTX2
by `texpack`, its glTF rewritten to use them, and an entry in `assets/manifest.json`.

    python3 tools/props/props.py                 # every model in UNITS
    python3 tools/props/props.py barrel_03 ...   # just these

Writes `assets/models/<id>/{model.gltf,model.bin,color.ktx2,normal.ktx2,orm.ktx2}`. The size of each
unit (length along +X, height, depth along +Z, in metres) is what the game's `PropVariant` for it
declares in `src/maps/map/mod.rs`; keep the two in step. Needs Blender (`BLENDER`, default
`/Applications/Blender.app/Contents/MacOS/Blender`) and the network. Sources go to a temporary
directory and are never committed.
"""

import datetime
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[2]
ASSETS = ROOT / "assets"
MANIFEST = ASSETS / "manifest.json"
API = "https://api.polyhaven.com"
BLENDER = os.environ.get("BLENDER", "/Applications/Blender.app/Contents/MacOS/Blender")
MAX_TRIS = 2000
# Said of the Refinery's models (#574), each of whose pages was read for its licence.
CC0_NOTE = "; licence read as CC0 1.0 (public domain dedication) on the model's Poly Haven page"

# id -> unit (length, height, depth), metres. The model is scaled to it per axis, never by more than
# ~20% from its modelled size, so the Props' +-15% fitting has room on top.
UNITS = {
    "barrel_03": (0.66, 0.95, 0.66),
    "Barrel_01": (0.66, 0.95, 0.66),
    "wine_barrel_01": (0.68, 0.95, 0.68),
    "painted_wooden_table": (2.5, 0.8, 1.1),
    "metal_office_desk": (2.0, 0.8, 1.0),
    "wooden_table_02": (1.1, 0.8, 0.8),
    "vintage_wooden_drawer_01": (1.4, 1.0, 0.85),
    "wooden_bookshelf_worn": (1.2, 2.3, 0.6),
    "painted_wooden_cabinet_02": (1.0, 2.3, 0.65),
    "worn_metal_rack": (0.95, 2.0, 0.6),
    # The Sanatorium's (#525).
    "old_bed_frame": (2.0, 1.1, 0.9),
    "wheelchair_01": (1.0, 1.0, 0.75),
    "tool_cart": (1.2, 0.95, 0.75),
    "steel_frame_shelves_01": (1.1, 2.1, 0.5),
    "drawer_cabinet": (1.1, 1.8, 0.5),
    "WetFloorSign_01": (0.36, 0.63, 0.3),
    "vintage_day_bed": (2.0, 1.1, 0.85),
    # The Archive's reading level (#540). The table is laid twice by the fitting as a 4.5 m reading table; the chairs and the
    # encyclopedias dress it (no collider of their own).
    "dining_table": (2.25, 0.88, 1.39),
    "SchoolChair_01": (0.68, 1.0, 0.57),
    "wooden_ladder": (0.96, 1.335, 0.5),
    "vintage_grandfather_clock_01": (1.0, 2.19, 0.8),
    "marble_bust_01": (0.3, 0.515, 0.27),
    "standing_chalkboard_01": (0.92, 1.51, 0.76),
    "book_encyclopedia_set_01": (0.55, 0.24, 0.16),
    "GothicCabinet_01": (1.72, 2.36, 1.13),
    # The Refinery's (#574): the jersey barrier, the utility box (the booth's cabinet, the control house's panels, the yards' cabinets), the
    # welding cart, the compressor and the jerrycan. Sizes are the models' own (within ~20%), turned to lie along +X.
    "concrete_road_barrier": (1.5, 0.84, 0.64),
    "utility_box_01": (0.52, 1.12, 0.43),
    "portable_welding_cart": (0.84, 1.56, 0.65),
    "old_military_compressor": (1.68, 1.18, 0.6),
    "metal_jerrycan": (0.36, 0.47, 0.18),
}

# The Refinery's units that are assembled from a catalogue model's pieces by `assemble.py` (#574): unit id -> (the Poly Haven model, the
# `assemble.py` unit, each material's texture prefix in the download, a name). Each material's textures go to `<material>_{color,normal,orm}.ktx2`.
ASSEMBLED = {
    # The pipe run wears the manifold's first material set, so the textures are on disk once.
    "pipe_manifold": ("modular_industrial_pipes_01", "pipe_manifold", {"g1": "modular_industrial_pipes_01_group01", "g2": "modular_industrial_pipes_01_group02"}, "Industrial pipe manifold"),
    "pipe_run": ("modular_industrial_pipes_01", "pipe_run", {"g1": ("share", "pipe_manifold")}, "Industrial pipe run"),
    # A thin cage and a near-black lamp pair need no textures: painted steel as flat glTF colours (saving ~7 MB of the budget).
    "fire_escape": ("modular_fire_escape", "fire_escape", {"g1": {"color": [0.16, 0.17, 0.17, 1.0], "metallic": 0.7, "roughness": 0.55}}, "Caged ladder"),
    "flood_lights": ("flood_lights", "flood_lights", {"g1": {"color": [0.04, 0.04, 0.045, 1.0], "metallic": 0.6, "roughness": 0.45}}, "Flood light pair"),
}


def get(url):
    request = urllib.request.Request(url, headers={"User-Agent": "zombies-props/1"})
    with urllib.request.urlopen(request) as response:
        return response.read()


def fetch(asset_id, workdir):
    """Download the 1K glTF and its textures into `workdir`; return the .gltf's path."""
    files = json.loads(get(f"{API}/files/{asset_id}"))["gltf"]["1k"]["gltf"]
    gltf = workdir / pathlib.Path(files["url"]).name
    gltf.write_bytes(get(files["url"]))
    for name, entry in files["include"].items():
        path = workdir / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(get(entry["url"]))
    return gltf


def texture(workdir, suffix, required=True):
    found = sorted((workdir / "textures").glob(f"*_{suffix}_1k.*"))
    if not found:
        if required:
            sys.exit(f"no {suffix} texture in {workdir}")
        return None
    return found[0]


def orm_args(workdir):
    """texpack's arguments for a model's occlusion-roughness-metal map: its `arm` texture, or, for a model that has only a
    roughness map (the marble bust), that, unoccluded and non-metallic."""
    arm = texture(workdir, "arm", required=False)
    if arm is not None:
        return ["--orm", str(arm)]
    return ["--roughness", str(texture(workdir, "rough"))]


def build(asset_id, unit):
    out = ASSETS / "models" / asset_id
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        source = tmp / "source"
        source.mkdir()
        gltf = fetch(asset_id, source)
        info = json.loads(get(f"{API}/info/{asset_id}"))
        work = tmp / "work"
        work.mkdir()
        run = subprocess.run(
            [BLENDER, "--background", "--python", str(ROOT / "tools/props/decimate.py"), "--",
             str(gltf), str(work / "model.gltf"), *map(str, unit), str(MAX_TRIS)],
            capture_output=True, text=True,
        )
        line = next((l for l in run.stdout.splitlines() if l.startswith("UNIT ")), None)
        if run.returncode != 0 or line is None:
            sys.exit(f"{asset_id}: Blender failed\n{run.stdout[-2000:]}\n{run.stderr[-2000:]}")
        shutil.rmtree(out, ignore_errors=True)
        out.mkdir(parents=True)
        subprocess.run(
            ["cargo", "run", "--quiet", "--release", "-p", "texpack", "--",
             "--color", str(texture(source, "diff")), "--normal", str(texture(source, "nor_gl")),
             *orm_args(source), "--out", str(out)],
            cwd=ROOT, check=True,
        )
        rough_only = texture(source, "arm", required=False) is None
        shutil.copy(work / "model.bin", out / "model.bin")
        doc = json.loads((work / "model.gltf").read_text())
        doc["buffers"][0]["uri"] = "model.bin"
        # The glTF refers to the KTX2 files as plain images: Bevy loads them by extension.
        doc["images"] = [{"uri": "color.ktx2"}, {"uri": "normal.ktx2"}, {"uri": "orm.ktx2"}]
        doc["samplers"] = [{"magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497}]
        doc["textures"] = [{"source": i, "sampler": 0} for i in range(3)]
        doc["materials"] = [{
            "name": asset_id,
            "pbrMetallicRoughness": {
                "baseColorTexture": {"index": 0},
                "metallicRoughnessTexture": {"index": 2},
                "metallicFactor": 1.0,
                "roughnessFactor": 1.0,
            },
            "normalTexture": {"index": 1},
            "occlusionTexture": {"index": 2},
        }]
        for mesh in doc["meshes"]:
            for primitive in mesh["primitives"]:
                primitive["material"] = 0
        (out / "model.gltf").write_text(json.dumps(doc, indent=1) + "\n")
    size = [float(x) for x in line.split()[1:4]]
    tris = int(re.search(r"tris=(\d+)", line).group(1))
    return {
        "path": f"models/{asset_id}",
        "name": info.get("name", asset_id),
        "source": f"https://polyhaven.com/a/{asset_id}",
        "licence": "CC0",
        "fetched": datetime.date.today().isoformat(),
        "notes": f"1K glTF decimated by tools/props to {tris} triangles, unit {size[0]:g} x {size[1]:g} x {size[2]:g} m (length, height, depth); textures as KTX2 via texpack" + ("; the model has only a roughness map, so its ORM is roughness alone (unoccluded, non-metal)" if rough_only else "") + CC0_NOTE,
    }


def textures_note(groups):
    flat = [m for m, spec in groups.items() if isinstance(spec, dict)]
    shared = [m for m, spec in groups.items() if isinstance(spec, tuple)]
    if flat:
        return "no textures: flat glTF colours (the model's are near-black steel)"
    if shared:
        return f"textures shared with models/{groups[shared[0]][1]} (KTX2 via texpack)"
    return f"textures as KTX2 via texpack ({len(groups)} material{'s' if len(groups) > 1 else ''})"


def build_assembled(unit_id):
    """One of `ASSEMBLED`: the model's pieces put together by Blender (`assemble.py`), each material's textures packed by texpack."""
    asset_id, script_unit, groups, name = ASSEMBLED[unit_id]
    out = ASSETS / "models" / unit_id
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        source = tmp / "source"
        source.mkdir()
        gltf = fetch(asset_id, source)
        info = json.loads(get(f"{API}/info/{asset_id}"))
        work = tmp / "work"
        work.mkdir()
        run = subprocess.run(
            [BLENDER, "--background", "--python", str(ROOT / "tools/props/assemble.py"), "--", script_unit, str(gltf), str(work / "model.gltf")],
            capture_output=True, text=True,
        )
        line = next((l for l in run.stdout.splitlines() if l.startswith("UNIT ")), None)
        if run.returncode != 0 or line is None:
            sys.exit(f"{unit_id}: Blender failed\n{run.stdout[-2000:]}\n{run.stderr[-2000:]}")
        shutil.rmtree(out, ignore_errors=True)
        out.mkdir(parents=True)
        images = []
        for material, spec in groups.items():
            if not isinstance(spec, str):
                continue
            found = {kind: sorted((source / "textures").glob(f"{spec}_{kind}_1k.*")) for kind in ("diff", "nor_gl", "arm")}
            if not all(found.values()):
                sys.exit(f"{unit_id}: textures missing for {spec}: {found}")
            pack = tmp / f"pack_{material}"
            pack.mkdir()
            subprocess.run(
                ["cargo", "run", "--quiet", "--release", "-p", "texpack", "--",
                 "--color", str(found["diff"][0]), "--normal", str(found["nor_gl"][0]), "--orm", str(found["arm"][0]), "--out", str(pack)],
                cwd=ROOT, check=True,
            )
            for kind in ("color", "normal", "orm"):
                shutil.copy(pack / f"{kind}.ktx2", out / f"{material}_{kind}.ktx2")
        shutil.copy(work / "model.bin", out / "model.bin")
        doc = json.loads((work / "model.gltf").read_text())
        doc["buffers"][0]["uri"] = "model.bin"
        by_name = {m["name"]: m for m in doc["materials"]}
        order = {m["name"]: i for i, m in enumerate(doc["materials"])}
        remap = {order[m]: i for i, m in enumerate(groups)}
        doc["materials"] = [by_name[m] for m in groups]
        for k, (name_, spec) in enumerate(groups.items()):
            material = doc["materials"][k]
            material.pop("extensions", None)
            material["doubleSided"] = False
            if isinstance(spec, dict):
                material["pbrMetallicRoughness"] = {"baseColorFactor": spec["color"], "metallicFactor": spec["metallic"], "roughnessFactor": spec["roughness"]}
                continue
            folder = "" if isinstance(spec, str) else f"../{spec[1]}/"
            first = len(images)
            images += [{"uri": f"{folder}{name_}_{kind}.ktx2"} for kind in ("color", "normal", "orm")]
            material["pbrMetallicRoughness"] = {
                "baseColorTexture": {"index": first},
                "metallicRoughnessTexture": {"index": first + 2},
                "metallicFactor": 1.0,
                "roughnessFactor": 1.0,
            }
            material["normalTexture"] = {"index": first + 1}
            material["occlusionTexture"] = {"index": first + 2}
        if images:
            doc["images"] = images
            doc["samplers"] = [{"magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497}]
            doc["textures"] = [{"source": i, "sampler": 0} for i in range(len(images))]
        for mesh in doc["meshes"]:
            for primitive in mesh["primitives"]:
                primitive["material"] = remap[primitive["material"]]
        (out / "model.gltf").write_text(json.dumps(doc, indent=1) + "\n")
    size = [float(x) for x in line.split()[1:4]]
    tris = int(re.search(r"tris=(\d+)", line).group(1))
    return {
        "path": f"models/{unit_id}",
        "name": f"{name} (assembled from Poly Haven {info.get('name', asset_id)})",
        "source": f"https://polyhaven.com/a/{asset_id}",
        "licence": "CC0",
        "fetched": datetime.date.today().isoformat(),
        "notes": f"1K glTF; a few of the model's pieces decimated, turned and joined by tools/props/assemble.py into one unit of {tris} triangles, {size[0]:g} x {size[1]:g} x {size[2]:g} m (length, height, depth), with a few boxes to carry them; " + textures_note(groups) + CC0_NOTE,
    }


def main():
    ids = sys.argv[1:] or [*UNITS, *ASSEMBLED]
    manifest = json.loads(MANIFEST.read_text())
    for asset_id in ids:
        entry = build_assembled(asset_id) if asset_id in ASSEMBLED else build(asset_id, UNITS[asset_id])
        manifest["assets"] = [e for e in manifest["assets"] if e["path"] != entry["path"]] + [entry]
        manifest["assets"].sort(key=lambda e: e["path"])
        MANIFEST.write_text(json.dumps(manifest, indent=2) + "\n")
        print(f"{asset_id}: {entry['notes']}")


if __name__ == "__main__":
    main()
