#!/usr/bin/env python3
"""Build the Hellhound asset end to end: the parts in headless Blender, a CC0 hide set as KTX2 with
`texpack`, the two joined, and the manifest entries written.

    python3 tools/hellhound/build.py [--blender blender] [--hide brown_leather]

Needs Blender on the PATH (or `--blender`), Rust (for `texpack`), and the network (Poly Haven).
Writes under `assets/models/hellhound/`:

    hellhound.gltf, hellhound.bin   the parts, from build_hellhound.py
    color.ktx2 normal.ktx2 orm.ktx2 the hide, BC7 / BC5 / BC7 as `tools/texpack/README.md` says

Bevy doesn't read `KHR_texture_basisu`, but it loads a texture by its file extension, so the glTF's
images simply name the `.ktx2` files. Hide texture ids are Poly Haven's. The model is never used by
tests: with it missing, the game draws its box hound.
"""

import argparse
import datetime
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = ROOT / "assets" / "models" / "hellhound"
MANIFEST = ROOT / "assets" / "manifest.json"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--blender", default="blender")
    parser.add_argument("--hide", default="brown_leather", help="Poly Haven texture id for the hide")
    args = parser.parse_args()
    if shutil.which(args.blender) is None:
        sys.exit(f"{args.blender}: not found. Install Blender (4.x) or pass --blender <path>")
    OUT.mkdir(parents=True, exist_ok=True)
    gltf = OUT / "hellhound.gltf"
    subprocess.run(
        [args.blender, "--background", "--python", str(ROOT / "tools/hellhound/build_hellhound.py"), "--", "--out", str(gltf)],
        check=True,
    )
    # The hide: Poly Haven -> texpack writes assets/materials/<id>/; move it beside the model.
    subprocess.run([sys.executable, str(ROOT / "tools/texpack/polyhaven.py"), args.hide], check=True)
    material = ROOT / "assets" / "materials" / args.hide
    for name in ("color", "normal", "orm"):
        shutil.copy(material / f"{name}.ktx2", OUT / f"{name}.ktx2")
    shutil.rmtree(material)
    texture(gltf)
    record(args.hide)


def texture(gltf):
    """Gives the glTF's Hide material the KTX2 hide: base colour, normal and ORM (occlusion in R,
    roughness in G, metallic in B), by file name."""
    doc = json.loads(gltf.read_text())
    doc["images"] = [{"uri": f"{name}.ktx2"} for name in ("color", "normal", "orm")]
    doc["samplers"] = [{"magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497}]
    doc["textures"] = [{"sampler": 0, "source": i} for i in range(3)]
    for material in doc["materials"]:
        if material["name"].startswith("Hide"):
            pbr = material.setdefault("pbrMetallicRoughness", {})
            pbr["baseColorTexture"] = {"index": 0}
            pbr["baseColorFactor"] = [1, 1, 1, 1]
            pbr["metallicRoughnessTexture"] = {"index": 2}
            pbr["metallicFactor"] = 1.0
            pbr["roughnessFactor"] = 1.0
            material["normalTexture"] = {"index": 1}
            material["occlusionTexture"] = {"index": 2}
    gltf.write_text(json.dumps(doc, indent=1) + "\n")


def record(hide):
    manifest = json.loads(MANIFEST.read_text())
    today = datetime.date.today().isoformat()
    manifest["assets"] = [e for e in manifest["assets"] if not e["path"].startswith("models/hellhound") and e["path"] != f"materials/{hide}"]
    manifest["assets"].append(
        {
            "path": "models/hellhound",
            "name": "Hellhound, rigid parts",
            "source": "tools/hellhound/build.py",
            "licence": "CC0 (made in this repo); hide texture CC0 from Poly Haven " + hide,
            "fetched": today,
            "notes": f"Our own Blender model (tools/hellhound/build_hellhound.py), hide from https://polyhaven.com/a/{hide} as KTX2 via texpack",
        }
    )
    manifest["assets"].sort(key=lambda entry: entry["path"])
    MANIFEST.write_text(json.dumps(manifest, indent=2) + "\n")


if __name__ == "__main__":
    main()
