#!/usr/bin/env python3
"""Fetch Poly Haven texture sets at 1K, turn each into the game's KTX2 files with `texpack`, and
record each in `assets/manifest.json`.

    python3 tools/texpack/polyhaven.py dark_wooden_planks damaged_plaster ...

Writes `assets/materials/<id>/{color,normal,orm}.ktx2` and prints each set's real-world size in
metres, which is the tile size its `MaterialDef` takes in `src/maps/map/textures.rs`.
Add `--res 2k` for a hero floor. Source PNGs go to a temporary directory and are never committed.
"""

import argparse
import datetime
import json
import pathlib
import subprocess
import sys
import tempfile
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[2]
ASSETS = ROOT / "assets"
MANIFEST = ASSETS / "manifest.json"
API = "https://api.polyhaven.com"
USER_AGENT = "zombies-texpack/1"


def get(url):
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request) as response:
        return response.read()


def get_json(url):
    return json.loads(get(url))


def load_manifest():
    if MANIFEST.exists():
        return json.loads(MANIFEST.read_text())
    return {"assets": []}


def save_manifest(manifest):
    manifest["assets"].sort(key=lambda entry: entry["path"])
    MANIFEST.write_text(json.dumps(manifest, indent=2) + "\n")


def fetch(asset_id, res, workdir):
    info = get_json(f"{API}/info/{asset_id}")
    if info.get("type") != 1:
        sys.exit(f"{asset_id}: not a Poly Haven texture")
    files = get_json(f"{API}/files/{asset_id}")
    maps = {}
    for key, name in (("Diffuse", "color"), ("nor_gl", "normal"), ("arm", "orm")):
        if key == "Diffuse" and key not in files:
            # Fabrics come as colourways: take the first.
            key = next(k for k in ("col_1", "col_01") if k in files)
        url = files[key][res]["png"]["url"]
        path = workdir / f"{asset_id}_{name}.png"
        path.write_bytes(get(url))
        maps[name] = path
    out = ASSETS / "materials" / asset_id
    subprocess.run(
        [
            "cargo", "run", "--quiet", "--release", "-p", "texpack", "--",
            "--color", str(maps["color"]),
            "--normal", str(maps["normal"]),
            "--orm", str(maps["orm"]),
            "--out", str(out),
        ],
        cwd=ROOT,
        check=True,
    )
    width_mm = info["dimensions"][0]
    return {
        "path": f"materials/{asset_id}",
        "name": info.get("name", asset_id),
        "source": f"https://polyhaven.com/a/{asset_id}",
        "licence": "CC0",
        "fetched": datetime.date.today().isoformat(),
        "notes": f"{res} PNG (Diffuse, nor_gl, arm); real-world size {width_mm / 1000:g} m",
    }, width_mm / 1000


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("ids", nargs="+")
    parser.add_argument("--res", default="1k")
    args = parser.parse_args()
    manifest = load_manifest()
    with tempfile.TemporaryDirectory() as tmp:
        for asset_id in args.ids:
            entry, size = fetch(asset_id, args.res, pathlib.Path(tmp))
            manifest["assets"] = [e for e in manifest["assets"] if e["path"] != entry["path"]] + [entry]
            save_manifest(manifest)
            print(f"{asset_id}: tile {size:g} m")


if __name__ == "__main__":
    main()
