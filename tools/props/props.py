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


def texture(workdir, suffix):
    found = sorted((workdir / "textures").glob(f"*_{suffix}_1k.*"))
    if not found:
        sys.exit(f"no {suffix} texture in {workdir}")
    return found[0]


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
             "--orm", str(texture(source, "arm")), "--out", str(out)],
            cwd=ROOT, check=True,
        )
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
        "notes": f"1K glTF decimated by tools/props to {tris} triangles, unit {size[0]:g} x {size[1]:g} x {size[2]:g} m (length, height, depth); textures as KTX2 via texpack",
    }


def main():
    ids = sys.argv[1:] or list(UNITS)
    manifest = json.loads(MANIFEST.read_text())
    for asset_id in ids:
        entry = build(asset_id, UNITS[asset_id])
        manifest["assets"] = [e for e in manifest["assets"] if e["path"] != entry["path"]] + [entry]
        manifest["assets"].sort(key=lambda e: e["path"])
        MANIFEST.write_text(json.dumps(manifest, indent=2) + "\n")
        print(f"{asset_id}: {entry['notes']}")


if __name__ == "__main__":
    main()
