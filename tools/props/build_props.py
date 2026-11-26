#!/usr/bin/env python3
"""Builds the kitbashed Prop units (Crate stacks, Sandbag courses, Headstone) as glTF and KTX2.

    python3 tools/props/build_props.py [--blender /path/to/Blender] [--only crate|sandbags|headstone]

What it does, in order:

1. Fetches the CC0 sources into a temporary directory: Poly Haven `wooden_crate_02` and
   `cement_bag` (1K glTF), and, through `tools/texpack/polyhaven.py`, the burlap `hessian_230`
   as a material under `assets/materials/`.
2. Runs `kitbash.py` in headless Blender: each unit comes out as one mesh of at most 2k triangles.
3. Turns the crate's textures into KTX2 with `texpack`, and points each glTF at its KTX2 files
   (a plain `uri` to a `.ktx2`, which Bevy loads by extension).
4. Records every source in `assets/manifest.json`.

Outputs, under `assets/props/`:

    crates/crate_stack_2.gltf, crate_stack_3.gltf (+ .bin) and color/normal/orm.ktx2
    sandbags/sandbags_3.gltf, sandbags_5.gltf (+ .bin), wearing ../../materials/hessian_230
    headstone/headstone.gltf (+ .bin) and its own KTX2, only if a scan could be fetched

The Headstone is a Sketchfab CC0 photogrammetry scan decimated to about 1.8k triangles with a
normal bake. Sketchfab only serves downloads to a signed-in account, so set `SKETCHFAB_TOKEN` (an
API token) to fetch it; without one the step is skipped and the game keeps the Headstone as its
textured box. `--headstone-scan <file>` bakes a scan you have already downloaded (glTF, GLB or OBJ),
recorded under `--headstone-source <page URL>`.

Re-running with the same sources writes the same files. Source downloads are never committed.
"""

import argparse
import datetime
import json
import os
import pathlib
import shutil
import struct
import subprocess
import sys
import tempfile
import zlib

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import fetch  # noqa: E402

ROOT = HERE.parents[1]
ASSETS = ROOT / "assets"
MANIFEST = ASSETS / "manifest.json"
BLENDER_DEFAULT = "/Applications/Blender.app/Contents/MacOS/Blender"
BURLAP = "hessian_230"
# A sandbag is a muddy khaki, not fresh burlap.
SANDBAG_TINT = [0.62, 0.58, 0.46, 1.0]
HEADSTONE_PAGE = "https://sketchfab.com/3d-models/drumgoon-hill-old-graveyard-headstone-537-ae0f3420458a4ecab0d4bcf3a69eab9c"
HEADSTONE_UID = "ae0f3420458a4ecab0d4bcf3a69eab9c"


def load_manifest():
    return json.loads(MANIFEST.read_text()) if MANIFEST.exists() else {"assets": []}


def record(entry):
    manifest = load_manifest()
    manifest["assets"] = [e for e in manifest["assets"] if e["path"] != entry["path"]] + [entry]
    manifest["assets"].sort(key=lambda e: e["path"])
    MANIFEST.write_text(json.dumps(manifest, indent=2) + "\n")


def today():
    return datetime.date.today().isoformat()


def texpack(color, normal, orm, out):
    subprocess.run(
        ["cargo", "run", "--quiet", "--release", "-p", "texpack", "--",
         "--color", str(color), "--normal", str(normal), "--orm", str(orm), "--out", str(out)],
        cwd=ROOT, check=True,
    )


def patch_gltf(path, uris, tint=None):
    """Gives the unit's one material its textures: `uris` is {color, normal, orm} relative to the
    glTF. The ORM image is both the occlusion and the metallic-roughness texture."""
    doc = json.loads(path.read_text())
    doc["images"] = [{"uri": uris[k]} for k in ("color", "normal", "orm")]
    doc["samplers"] = [{"magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497}]
    doc["textures"] = [{"sampler": 0, "source": i} for i in range(3)]
    (material,) = doc["materials"]
    material["doubleSided"] = False
    pbr = material["pbrMetallicRoughness"]
    pbr["baseColorTexture"] = {"index": 0}
    pbr["metallicRoughnessTexture"] = {"index": 2}
    pbr["metallicFactor"] = 1.0
    pbr["roughnessFactor"] = 1.0
    if tint:
        pbr["baseColorFactor"] = tint
    material["normalTexture"] = {"index": 1}
    material["occlusionTexture"] = {"index": 2}
    path.write_text(json.dumps(doc, indent=1) + "\n")


def run_blender(blender, args):
    result = subprocess.run(
        [blender, "-b", "--factory-startup", "--python", str(HERE / "kitbash.py"), "--", *map(str, args)],
        check=True, capture_output=True, text=True,
    )
    for line in result.stdout.splitlines():
        if line.startswith("UNIT "):
            print(line[5:])


def png(path, width, height, rgb):
    """A flat-colour PNG, stdlib only."""
    def chunk(tag, data):
        body = tag + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))
    row = b"\x00" + bytes(rgb) * width
    data = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
    data += chunk(b"IDAT", zlib.compress(row * height)) + chunk(b"IEND", b"")
    pathlib.Path(path).write_bytes(data)


def build_crates(blender, src, work):
    out = ASSETS / "props" / "crates"
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    run_blender(blender, ["--src", src, "--out", out, "--only", "crate"])
    tex = src / "wooden_crate_02" / "textures"
    texpack(tex / "wooden_crate_02_diff_1k.jpg", tex / "wooden_crate_02_nor_gl_1k.jpg", tex / "wooden_crate_02_arm_1k.jpg", out)
    uris = {k: f"{k}.ktx2" for k in ("color", "normal", "orm")}
    for gltf in sorted(out.glob("*.gltf")):
        patch_gltf(gltf, uris)
    record({
        "path": "props/crates",
        "name": "Crate stacks (kitbash of Poly Haven Wooden Crate 02)",
        "source": "https://polyhaven.com/a/wooden_crate_02",
        "licence": "CC0",
        "fetched": today(),
        "notes": "2- and 3-high stacks of wooden_crate_02 (1K glTF), each crate decimated to ~325 triangles and stretched to a 0.53 m layer, joined into one mesh by tools/props/kitbash.py. Textures are the model's 1K diffuse, nor_gl and arm via texpack",
    })


def build_sandbags(blender, src, work):
    out = ASSETS / "props" / "sandbags"
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    if not (ASSETS / "materials" / BURLAP / "color.ktx2").exists():
        subprocess.run([sys.executable, str(ROOT / "tools" / "texpack" / "polyhaven.py"), BURLAP], cwd=ROOT, check=True)
    run_blender(blender, ["--src", src, "--out", out, "--only", "sandbags"])
    uris = {k: f"../../materials/{BURLAP}/{k}.ktx2" for k in ("color", "normal", "orm")}
    for gltf in sorted(out.glob("*.gltf")):
        patch_gltf(gltf, uris, SANDBAG_TINT)
    record({
        "path": "props/sandbags",
        "name": "Sandbag courses (kitbash of Poly Haven Cement Bag)",
        "source": "https://polyhaven.com/a/cement_bag",
        "licence": "CC0",
        "fetched": today(),
        "notes": f"3- and 5-course walls of cement_bag (1K glTF) decimated to ~165 triangles a bag and joined into one mesh by tools/props/kitbash.py. The sack's branded texture is dropped: its UVs are rebuilt as box-projected burlap and it wears materials/{BURLAP} (Poly Haven, CC0), tinted khaki",
    })


def sketchfab_scan(work):
    """Downloads the Headstone scan with `SKETCHFAB_TOKEN`; None if there is no token or the
    download fails."""
    token = os.environ.get("SKETCHFAB_TOKEN")
    if not token:
        return None
    import io
    import urllib.request
    import zipfile
    try:
        info = fetch.get_json(f"https://api.sketchfab.com/v3/models/{HEADSTONE_UID}")
        if info.get("license", {}).get("slug") != "cc0":
            print(f"headstone scan is not CC0 ({info.get('license')}): skipping", file=sys.stderr)
            return None
        request = urllib.request.Request(
            f"https://api.sketchfab.com/v3/models/{HEADSTONE_UID}/download",
            headers={"Authorization": f"Token {token}", "User-Agent": fetch.USER_AGENT},
        )
        with urllib.request.urlopen(request, timeout=120) as response:
            url = json.load(response)["gltf"]["url"]
        archive = zipfile.ZipFile(io.BytesIO(fetch.get(url)))
        directory = pathlib.Path(work) / "headstone_scan"
        archive.extractall(directory)
        return next(directory.rglob("*.gltf"))
    except Exception as err:  # the box is the fallback for any failure
        print(f"headstone scan not fetched: {err}", file=sys.stderr)
        return None


def build_headstone(blender, work, scan, source):
    scan = pathlib.Path(scan) if scan else sketchfab_scan(work)
    if scan is None:
        print("headstone: no scan, so the Headstone stays its textured box")
        return
    out = ASSETS / "props" / "headstone"
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    baked = pathlib.Path(work) / "headstone_baked"
    run_blender(blender, ["--src", work, "--out", out, "--headstone", scan, "--work", baked])
    orm = baked / "headstone_orm.png"
    png(orm, 4, 4, (255, 224, 0))  # unoccluded, rough, not metal
    texpack(baked / "headstone_color.png", baked / "headstone_normal.png", orm, out)
    patch_gltf(out / "headstone.gltf", {k: f"{k}.ktx2" for k in ("color", "normal", "orm")})
    record({
        "path": "props/headstone",
        "name": "Headstone (decimated Sketchfab scan)",
        "source": source or HEADSTONE_PAGE,
        "licence": "CC0 (as stated on the model's Sketchfab page)",
        "fetched": today(),
        "notes": "Photogrammetry scan decimated to ~1.8k triangles with a 1K normal and base-colour bake, by tools/props/kitbash.py",
    })


# The Sanatorium's kitbashes (#525): each is built from primitives by `kitbash.py`, one mesh per unit, wearing
# material sets already under assets/materials. unit -> (name, {material set: base-colour tint}).
SANATORIUM_KITS = {
    "gurney": ("Gurney", {"rusty_metal_02": None, "fabric_pattern_07": [0.78, 0.8, 0.74, 1.0]}),
    "dressing_trolley": ("Dressing trolley", {"rusty_metal_02": None, "metal_plate_02": None}),
    "locker_bank": ("Locker bank", {"painted_metal_shutter": [0.66, 0.72, 0.6, 1.0], "rusty_metal_02": None}),
    "cot": ("Cot", {"dark_wooden_planks": None, "fabric_pattern_07": [0.82, 0.84, 0.78, 1.0]}),
    "mortuary_slab": ("Mortuary slab", {"metal_plate_02": None, "rusty_metal_02": None}),
    "bath_tub": ("Bath tub", {"long_white_tiles": [0.86, 0.9, 0.86, 1.0], "rusty_metal_02": None}),
    "rocking_horse": ("Rocking horse", {"dark_wooden_planks": None, "fabric_pattern_07": [0.7, 0.55, 0.5, 1.0]}),
    "ambulance": ("Derelict ambulance", {"green_metal_rust": None, "rusty_metal_02": [0.28, 0.28, 0.28, 1.0]}),
}


def patch_sets(path, tints):
    """Gives each of a unit's materials (named for a material set) that set's textures, from
    `assets/materials/<set>`, with its tint."""
    doc = json.loads(path.read_text())
    sets = list(tints)
    doc["images"] = [{"uri": f"../../materials/{name}/{kind}.ktx2"} for name in sets for kind in ("color", "normal", "orm")]
    doc["samplers"] = [{"magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497}]
    doc["textures"] = [{"sampler": 0, "source": i} for i in range(len(doc["images"]))]
    for material in doc["materials"]:
        k = sets.index(material["name"])
        material["doubleSided"] = False
        pbr = material["pbrMetallicRoughness"]
        pbr["baseColorTexture"] = {"index": 3 * k}
        pbr["metallicRoughnessTexture"] = {"index": 3 * k + 2}
        pbr["metallicFactor"] = 1.0
        pbr["roughnessFactor"] = 1.0
        if tints[material["name"]]:
            pbr["baseColorFactor"] = tints[material["name"]]
        material["normalTexture"] = {"index": 3 * k + 1}
        material["occlusionTexture"] = {"index": 3 * k + 2}
    path.write_text(json.dumps(doc, indent=1) + "\n")


def build_sanatorium(blender, only):
    for unit, (name, tints) in SANATORIUM_KITS.items():
        if only not in (None, "sanatorium", unit):
            continue
        out = ASSETS / "props" / unit
        shutil.rmtree(out, ignore_errors=True)
        out.mkdir(parents=True)
        run_blender(blender, ["--out", out, "--src", out, "--only", unit])
        patch_sets(out / f"{unit}.gltf", tints)
        record({
            "path": f"props/{unit}",
            "name": f"{name} (kitbash for the Sanatorium)",
            "source": "tools/props/kitbash.py, from " + ", ".join(f"https://polyhaven.com/a/{s}" for s in tints),
            "licence": "CC0",
            "fetched": today(),
            "notes": "Modelled from boxes and cylinders by tools/props/kitbash.py in headless Blender (at most 2k triangles); it wears materials/" + " and materials/".join(tints) + " (Poly Haven, CC0), no new texture files",
        })


def build_book_rows(blender):
    """The Archive's shelf fill (#537): three variants of one face of book spines, each one mesh in
    per-vertex library colours over the `book_pattern` cloth. One manifest entry covers the directory."""
    out = ASSETS / "props" / "book_rows"
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    for variant in ("a", "b", "c"):
        run_blender(blender, ["--out", out, "--src", out, "--only", f"book_rows_{variant}"])
        patch_sets(out / f"book_rows_{variant}.gltf", {"book_pattern": [1.0, 1.0, 1.0, 1.0]})
    record({
        "path": "props/book_rows",
        "name": "Book rows (kitbash for the Archive's Bookcases and fixed stacks)",
        "source": "tools/props/kitbash.py, from https://polyhaven.com/a/book_pattern",
        "licence": "CC0",
        "fetched": today(),
        "notes": "Three variants (a, b mirrored, c offset) of one face of book spines, front and top faces only, 6 shelves of about 50 spines, built by tools/props/kitbash.py in headless Blender (about 1.1k triangles each) with a 12-colour library palette in per-vertex colour; it wears materials/book_pattern (Poly Haven, CC0), no new texture files",
    })


# The Archive's Wheels (#538): three kitbashes (post, hand wheel, chain) in cast iron and brass over material sets already
# under assets/materials. unit -> (name, {material set: base-colour tint}).
WHEEL_KITS = {
    "wheel_post": ("Wheel post", {"rusty_metal_02": [0.3, 0.3, 0.32, 1.0], "metal_plate_02": [0.75, 0.55, 0.25, 1.0]}),
    "aisle_wheel": ("Aisle wheel", {"rusty_metal_02": [0.3, 0.3, 0.32, 1.0], "metal_plate_02": [0.75, 0.55, 0.25, 1.0]}),
    "wheel_chain": ("Wheel chain", {"rusty_metal_02": [0.22, 0.22, 0.24, 1.0], "metal_plate_02": [0.75, 0.55, 0.25, 1.0]}),
}


def build_wheels(blender, only):
    for unit, (name, tints) in WHEEL_KITS.items():
        if only not in (None, "wheels", unit):
            continue
        out = ASSETS / "props" / unit
        shutil.rmtree(out, ignore_errors=True)
        out.mkdir(parents=True)
        run_blender(blender, ["--out", out, "--src", out, "--only", unit])
        patch_sets(out / f"{unit}.gltf", tints)
        record({
            "path": f"props/{unit}",
            "name": f"{name} (kitbash for the Archive's Ranges)",
            "source": "tools/props/kitbash.py, from " + ", ".join(f"https://polyhaven.com/a/{s}" for s in tints),
            "licence": "CC0",
            "fetched": today(),
            "notes": "Modelled from boxes and cylinders by tools/props/kitbash.py in headless Blender (at most 2k triangles); it wears materials/" + " and materials/".join(tints) + " (Poly Haven, CC0), no new texture files. Not fitted into a Prop's box: src/maps/stacks_look.rs stands it at a Wheel",
        })


# The Refinery's Valve (#572): a stand, a hand wheel and a gauge needle in `rusty_metal_02` and flat colours. unit -> (name, {material: spec}).
VALVE_KITS = {
    "valve_stand": ("Valve stand", {
        "rusty_metal_02": [0.28, 0.27, 0.27, 1.0],
        "hazard_yellow": {"color": [0.9, 0.7, 0.05, 1.0], "metallic": 0.0, "roughness": 0.6},
        "hazard_black": {"color": [0.03, 0.03, 0.03, 1.0], "metallic": 0.0, "roughness": 0.7},
        "gauge_white": {"color": [0.92, 0.92, 0.88, 1.0], "metallic": 0.0, "roughness": 0.4},
    }),
    "valve_wheel": ("Valve hand wheel", {
        "valve_red": {"color": [0.7, 0.05, 0.04, 1.0], "metallic": 0.3, "roughness": 0.5},
        "rusty_metal_02": [0.28, 0.27, 0.27, 1.0],
    }),
    "valve_needle": ("Valve gauge needle", {"needle_red": {"color": [0.9, 0.03, 0.02, 1.0], "metallic": 0.0, "roughness": 0.4}}),
}


def build_valves(blender, only):
    for unit, (name, specs) in VALVE_KITS.items():
        if only not in (None, "valves", unit):
            continue
        out = ASSETS / "props" / unit
        shutil.rmtree(out, ignore_errors=True)
        out.mkdir(parents=True)
        run_blender(blender, ["--out", out, "--src", out, "--only", unit])
        patch_mixed(out / f"{unit}.gltf", specs)
        sets = [m for m, spec in specs.items() if not isinstance(spec, dict)]
        record({
            "path": f"props/{unit}",
            "name": f"{name} (kitbash for the Refinery's Fuel Lines)",
            "source": "tools/props/kitbash.py" + "".join(f", from https://polyhaven.com/a/{s}" for s in sets),
            "licence": "CC0",
            "fetched": today(),
            "notes": "Modelled from boxes and cylinders by tools/props/kitbash.py in headless Blender (at most 2k triangles); "
            + ("it wears materials/rusty_metal_02 (Poly Haven, CC0) and flat glTF colours, no new texture files" if sets else "flat glTF colours only, no texture files")
            + ". Not fitted into a Prop's box: src/maps/fire_look.rs stands it at each Valve, turning the wheel and the needle and drawing the lamp",
        })


# The Archive's reading-level kitbashes (#540): each wears material sets already under assets/materials (tinted) and, for its glass,
# its lamp and its globe's ink, flat colours with no texture. unit -> (name, {material: spec}), where a spec is a tint (a set from
# assets/materials) or a dict of glTF factors (a flat colour): `color`, `metallic`, `roughness`, and optionally `emissive` (a linear
# colour), `strength` (KHR_materials_emissive_strength) and `alpha` (blended).
BRASS = [0.75, 0.55, 0.25, 1.0]
ARCHIVE_KITS = {
    "card_catalogue": ("Card catalogue", {"wood_table_worn": [0.7, 0.62, 0.55, 1.0], "dark_paneled_wood": None, "metal_plate_02": BRASS}),
    "issue_desk": ("Issue desk", {"wood_table_worn": [0.7, 0.62, 0.55, 1.0], "dark_paneled_wood": None, "metal_plate_02": BRASS}),
    "reading_lamp": ("Reading lamp", {
        "metal_plate_02": BRASS,
        "lamp_shade": {"color": [0.05, 0.3, 0.12, 1.0], "metallic": 0.0, "roughness": 0.3, "emissive": [0.3, 0.9, 0.45], "strength": 3.0},
        "lamp_bulb": {"color": [1.0, 0.8, 0.5, 1.0], "metallic": 0.0, "roughness": 0.4, "emissive": [1.0, 0.78, 0.45], "strength": 10.0},
    }),
    "lectern": ("Lectern", {"wood_table_worn": [0.7, 0.62, 0.55, 1.0], "dark_paneled_wood": None, "metal_plate_02": BRASS}),
    "theatre_bench": ("Theatre tiers", {"wood_table_worn": [0.7, 0.62, 0.55, 1.0], "dark_paneled_wood": None, "brown_leather": [0.7, 0.45, 0.35, 1.0], "metal_plate_02": BRASS}),
    "projector": ("Lantern-slide projector", {"wood_table_worn": [0.7, 0.62, 0.55, 1.0], "metal_plate_02": BRASS, "rusty_metal_02": [0.3, 0.3, 0.32, 1.0]}),
    "display_case": ("Display case", {
        "wood_table_worn": [0.7, 0.62, 0.55, 1.0], "dark_paneled_wood": None, "paper006": [0.9, 0.85, 0.75, 1.0], "metal_plate_02": BRASS,
        "glass": {"color": [0.75, 0.88, 0.9, 0.18], "metallic": 0.0, "roughness": 0.05, "alpha": True},
    }),
    "globe": ("Globe", {
        "paper006": [0.85, 0.62, 0.38, 1.0], "wood_table_worn": [0.55, 0.45, 0.4, 1.0], "metal_plate_02": BRASS,
        "ink": {"color": [0.1, 0.06, 0.03, 1.0], "metallic": 0.0, "roughness": 0.8},
    }),
}


def patch_mixed(path, specs):
    """Gives each of a unit's materials either a set's textures from `assets/materials/<set>` and its tint, or (for a flat colour) the
    glTF factors in its spec dict: a colour, metallic and roughness, an emissive colour and strength, and blending."""
    doc = json.loads(path.read_text())
    sets = [name for name, spec in specs.items() if not isinstance(spec, dict) and any(m["name"] == name for m in doc["materials"])]
    doc["images"] = [{"uri": f"../../materials/{name}/{kind}.ktx2"} for name in sets for kind in ("color", "normal", "orm")]
    doc["samplers"] = [{"magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497}]
    doc["textures"] = [{"sampler": 0, "source": i} for i in range(len(doc["images"]))]
    for material in doc["materials"]:
        spec = specs[material["name"]]
        material["doubleSided"] = False
        pbr = material["pbrMetallicRoughness"]
        if isinstance(spec, dict):
            pbr.pop("baseColorTexture", None)
            pbr["baseColorFactor"] = spec["color"]
            pbr["metallicFactor"] = spec["metallic"]
            pbr["roughnessFactor"] = spec["roughness"]
            if "emissive" in spec:
                material["emissiveFactor"] = spec["emissive"]
                if "strength" in spec:
                    material.setdefault("extensions", {})["KHR_materials_emissive_strength"] = {"emissiveStrength": spec["strength"]}
                    if "KHR_materials_emissive_strength" not in doc.setdefault("extensionsUsed", []):
                        doc["extensionsUsed"].append("KHR_materials_emissive_strength")
            if spec.get("alpha"):
                material["alphaMode"] = "BLEND"
            continue
        k = sets.index(material["name"])
        pbr["baseColorTexture"] = {"index": 3 * k}
        pbr["metallicRoughnessTexture"] = {"index": 3 * k + 2}
        pbr["metallicFactor"] = 1.0
        pbr["roughnessFactor"] = 1.0
        if spec:
            pbr["baseColorFactor"] = spec
        material["normalTexture"] = {"index": 3 * k + 1}
        material["occlusionTexture"] = {"index": 3 * k + 2}
    path.write_text(json.dumps(doc, indent=1) + "\n")


# The Archive's stacks-and-cellar kitbashes (#541): the same kind of spec, `--only archive_stacks` for all nine.
IRON_GREY = [0.3, 0.3, 0.32, 1.0]
WOOD_WORN = [0.7, 0.62, 0.55, 1.0]
ARCHIVE_STACKS_KITS = {
    "book_trolley": ("Book trolley", {"wood_table_worn": WOOD_WORN, "dark_paneled_wood": None, "brown_leather": [0.7, 0.45, 0.35, 1.0], "paper006": [0.9, 0.85, 0.75, 1.0], "rusty_metal_02": IRON_GREY}),
    "book_pile": ("Book pile", {"brown_leather": [0.7, 0.45, 0.35, 1.0], "paper006": [0.9, 0.85, 0.75, 1.0]}),
    "book_press": ("Book press", {"wood_table_worn": WOOD_WORN, "dark_paneled_wood": None, "rusty_metal_02": IRON_GREY, "paper006": [0.9, 0.85, 0.75, 1.0], "metal_plate_02": BRASS}),
    "guillotine": ("Guillotine (paper cutter)", {"wood_table_worn": WOOD_WORN, "dark_paneled_wood": None, "rusty_metal_02": IRON_GREY, "paper006": [0.9, 0.85, 0.75, 1.0], "metal_plate_02": BRASS}),
    "safe": ("Safe", {"rusty_metal_02": IRON_GREY, "dark_paneled_wood": None, "metal_plate_02": BRASS}),
    "deed_boxes": ("Deed boxes", {"green_metal_rust": [0.35, 0.38, 0.36, 1.0], "metal_plate_02": BRASS, "paper006": [0.9, 0.85, 0.75, 1.0], "wood_table_worn": WOOD_WORN}),
    "pump_engine": ("Pump engine", {"green_metal_rust": None, "rusty_metal_02": IRON_GREY, "metal_plate_02": BRASS}),
    "bucket": ("Bucket", {"metal_plate_02": [0.6, 0.62, 0.64, 1.0]}),
    "candles": ("Candles", {"metal_plate_02": BRASS, "candle_wax": {"color": [0.9, 0.84, 0.66, 1.0], "metallic": 0.0, "roughness": 0.6}}),
}


def build_archive(blender, only):
    for unit, (name, specs) in {**ARCHIVE_KITS, **ARCHIVE_STACKS_KITS}.items():
        if only not in (None, "archive", unit) and not (only == "archive_stacks" and unit in ARCHIVE_STACKS_KITS):
            continue
        if only == "archive" and unit in ARCHIVE_STACKS_KITS:
            continue
        out = ASSETS / "props" / unit
        shutil.rmtree(out, ignore_errors=True)
        out.mkdir(parents=True)
        run_blender(blender, ["--out", out, "--src", out, "--only", unit])
        patch_mixed(out / f"{unit}.gltf", specs)
        sets = [m for m, spec in specs.items() if not isinstance(spec, dict)]
        record({
            "path": f"props/{unit}",
            "name": f"{name} (kitbash for the Archive)",
            "source": "tools/props/kitbash.py, from " + ", ".join(f"https://polyhaven.com/a/{s}" if s != "paper006" else "https://ambientcg.com/a/Paper006" for s in sets),
            "licence": "CC0",
            "fetched": today(),
            "notes": "Modelled from boxes and cylinders by tools/props/kitbash.py in headless Blender (at most 2k triangles); it wears materials/" + " and materials/".join(sets) + " (Poly Haven and ambientCG, CC0), no new texture files; its flat colours are glTF factors"
            + (", its lamp emissive (KHR_materials_emissive_strength)" if unit == "reading_lamp" else "")
            + ("; the candle flames are drawn in code (src/maps/map/garnish.rs)" if unit == "candles" else "")
            + (". No CC0 globe exists (#514): the sphere is Paper006 tinted sepia with a lat/long graticule modelled as thin ink quads, no continents" if unit == "globe" else ""),
        })


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--blender", default=os.environ.get("BLENDER", BLENDER_DEFAULT))
    parser.add_argument("--only", choices=["crate", "sandbags", "headstone", "sanatorium", *SANATORIUM_KITS, "book_rows", "wheels", *WHEEL_KITS, "valves", *VALVE_KITS, "archive", "archive_stacks", *ARCHIVE_KITS, *ARCHIVE_STACKS_KITS])
    parser.add_argument("--headstone-scan")
    parser.add_argument("--headstone-source")
    args = parser.parse_args()
    if args.only == "book_rows":
        build_book_rows(args.blender)
        return
    if args.only in ("archive", "archive_stacks", *ARCHIVE_KITS, *ARCHIVE_STACKS_KITS):
        build_archive(args.blender, args.only)
        return
    if args.only in ("wheels", *WHEEL_KITS):
        build_wheels(args.blender, args.only)
        return
    if args.only in ("valves", *VALVE_KITS):
        build_valves(args.blender, args.only)
        return
    if args.only in ("sanatorium", *SANATORIUM_KITS):
        build_sanatorium(args.blender, args.only)
        return
    with tempfile.TemporaryDirectory() as tmp:
        work = pathlib.Path(tmp)
        src = work / "src"
        src.mkdir()
        if args.only in (None, "crate"):
            fetch.polyhaven_model("wooden_crate_02", src)
            build_crates(args.blender, src, work)
        if args.only in (None, "sandbags"):
            fetch.polyhaven_model("cement_bag", src)
            build_sandbags(args.blender, src, work)
        if args.only in (None, "headstone"):
            build_headstone(args.blender, work, args.headstone_scan, args.headstone_source)


if __name__ == "__main__":
    main()
