#!/usr/bin/env python3
"""Fetch ambientCG material sets at 1K, turn each into the game's KTX2 files with `texpack`, and
record each in `assets/manifest.json`.

    python3 tools/texpack/ambientcg.py PaintedPlaster003 Tiles133D --tiles 4 --tile-m 0.3
    python3 tools/texpack/ambientcg.py Sign004 --decal

Downloads the `1K-PNG` zip, packs `AmbientOcclusion`, `Roughness` and `Metalness` into the ORM the
way `polyhaven.py` does (texpack does the packing), uses the `NormalGL` map, and writes
`assets/materials/<id>/{color,normal,orm}.ktx2`. It prints the set's real-world size in metres,
which is the tile size its `Material` takes in `src/maps/map/textures.rs`. The size is the API's
`dimensionX` where it gives one; most photo scans don't, so give the size otherwise, either as
`--size <metres>` or measured from the tiles, `--tiles <how many tiles fit across> --tile-m <one
tile's width, in metres>`. With `--decal` the set is a decal: `assets/decals/<lowercase id>/color.ktx2`,
the colour map with its `Opacity` as alpha.

Licence: every ambientCG asset is CC0 (https://docs.ambientcg.com/license/). The manifest `source` is
the asset URL, and the notes cite that page, as the individual asset pages carry no licence label.

The zip is untrusted data. It is downloaded into a new empty temporary directory and only the named
maps are read from it, each by its exact member name and under a size cap; nothing is extracted by
path and nothing in it is run. Run this script as `python3 -I` if the working directory isn't trusted.
"""

import argparse
import datetime
import json
import pathlib
import subprocess
import sys
import tempfile
import urllib.parse
import urllib.request
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
ASSETS = ROOT / "assets"
MANIFEST = ASSETS / "manifest.json"
API = "https://ambientcg.com/api/v2/full_json"
LICENCE_PAGE = "https://docs.ambientcg.com/license/"
USER_AGENT = "zombies-texpack/1"
# A 1K PNG map is at most a few MB; refuse anything claiming much more.
MAX_MEMBER_BYTES = 64 * 1000 * 1000
MAX_ZIP_BYTES = 200 * 1000 * 1000


def get(url, limit):
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request) as response:
        data = response.read(limit + 1)
    if len(data) > limit:
        sys.exit(f"{url}: larger than {limit} bytes")
    return data


def load_manifest():
    if MANIFEST.exists():
        return json.loads(MANIFEST.read_text())
    return {"assets": []}


def save_manifest(manifest):
    manifest["assets"].sort(key=lambda entry: entry["path"])
    MANIFEST.write_text(json.dumps(manifest, indent=2) + "\n")


def asset_info(asset_id):
    query = urllib.parse.urlencode({"id": asset_id, "include": "downloadData,dimensionsData", "limit": 1})
    found = json.loads(get(f"{API}?{query}", 5_000_000))["foundAssets"]
    if not found or found[0]["assetId"] != asset_id:
        sys.exit(f"{asset_id}: not on ambientCG")
    return found[0]


def zip_url(info):
    """The `1K-PNG` zip's link, from the API's own listing."""
    for folder in info["downloadFolders"].values():
        for download in folder["downloadFiletypeCategories"].get("zip", {}).get("downloads", []):
            if download["attribute"] == "1K-PNG":
                return download["downloadLink"]
    sys.exit(f"{info['assetId']}: no 1K-PNG download")


def read_maps(archive, asset_id, names, workdir):
    """Copy the named maps out of the zip into `workdir`, by exact member name."""
    maps = {}
    with zipfile.ZipFile(archive) as z:
        members = {m.filename: m for m in z.infolist()}
        for key, optional in names:
            member = members.get(f"{asset_id}_1K-PNG_{key}.png")
            if member is None:
                if optional:
                    continue
                sys.exit(f"{asset_id}: the zip has no {key} map")
            if member.file_size > MAX_MEMBER_BYTES:
                sys.exit(f"{asset_id}: {key} claims {member.file_size} bytes")
            path = workdir / f"{key}.png"
            with z.open(member) as source:
                data = source.read(MAX_MEMBER_BYTES + 1)
            if len(data) > MAX_MEMBER_BYTES:
                sys.exit(f"{asset_id}: {key} is larger than it claimed")
            path.write_bytes(data)
            maps[key] = path
    return maps


def texpack(*args):
    subprocess.run(["cargo", "run", "--quiet", "--release", "-p", "texpack", "--", *args], cwd=ROOT, check=True)


def real_size(info, args):
    """The tile size in metres: given, else the API's `dimensionX` (in cm), else measured from the tiles."""
    if args.size:
        return args.size
    if info.get("dimensionX"):
        return info["dimensionX"] / 100
    if args.tiles and args.tile_m:
        return args.tiles * args.tile_m
    return None


def fetch(asset_id, args, workdir):
    info = asset_info(asset_id)
    archive = workdir / "download.zip"
    archive.write_bytes(get(zip_url(info), MAX_ZIP_BYTES))
    extracted = workdir / "maps"
    extracted.mkdir()
    entry = {
        "name": f"ambientCG {asset_id}",
        "source": f"https://ambientcg.com/a/{asset_id}",
        "licence": "CC0",
        "fetched": datetime.date.today().isoformat(),
    }
    if args.decal:
        maps = read_maps(archive, asset_id, [("Color", False), ("Opacity", False)], extracted)
        out = ASSETS / "decals" / asset_id.lower()
        texpack("--color", str(maps["Color"]), "--opacity", str(maps["Opacity"]), "--out", str(out))
        entry["path"] = f"decals/{asset_id.lower()}"
        entry["notes"] = f"1K PNG Color with Opacity as alpha, via texpack. CC0 per the site licence, {LICENCE_PAGE}"
        return entry, None
    size = real_size(info, args)
    if size is None:
        sys.exit(f"{asset_id}: the API gives no size; pass --size <metres> or --tiles <n> --tile-m <metres>")
    maps = read_maps(
        archive,
        asset_id,
        [("Color", False), ("NormalGL", False), ("Roughness", False), ("AmbientOcclusion", True), ("Metalness", True)],
        extracted,
    )
    out = ASSETS / "materials" / asset_id.lower()
    flags = ["--color", str(maps["Color"]), "--normal", str(maps["NormalGL"]), "--roughness", str(maps["Roughness"])]
    for key, flag in (("AmbientOcclusion", "--ao"), ("Metalness", "--metalness")):
        if key in maps:
            flags += [flag, str(maps[key])]
    texpack(*flags, "--out", str(out))
    entry["path"] = f"materials/{asset_id.lower()}"
    entry["notes"] = (
        f"1K PNG (Color, NormalGL, AmbientOcclusion, Roughness, Metalness packed to an ORM); real-world size {size:g} m. "
        f"CC0 per the site licence, {LICENCE_PAGE}"
    )
    return entry, size


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("ids", nargs="+")
    parser.add_argument("--decal", action="store_true", help="make a decal (Color with Opacity as alpha), not a material")
    parser.add_argument("--size", type=float, help="the tile size in metres, when the API gives none")
    parser.add_argument("--tiles", type=float, help="how many tiles fit across one repeat of the image")
    parser.add_argument("--tile-m", type=float, help="one tile's width in metres, with --tiles")
    args = parser.parse_args()
    if len(args.ids) > 1 and (args.size or args.tiles):
        sys.exit("--size and --tiles apply to one id at a time")
    manifest = load_manifest()
    for asset_id in args.ids:
        # Each download gets its own new, empty directory.
        with tempfile.TemporaryDirectory() as tmp:
            entry, size = fetch(asset_id, args, pathlib.Path(tmp))
        manifest["assets"] = [e for e in manifest["assets"] if e["path"] != entry["path"]] + [entry]
        save_manifest(manifest)
        print(f"{asset_id}: " + (f"tile {size:g} m" if size else "decal"))


if __name__ == "__main__":
    main()
