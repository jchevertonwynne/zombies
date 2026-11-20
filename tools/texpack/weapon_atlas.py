#!/usr/bin/env python3
"""Build the shared weapon atlas: one 1K image of the finishes every code-built gun, hand and sleeve
is drawn in, packed to `assets/weapons/atlas/{color,normal,orm}.ktx2` with `texpack`.

    python3 tools/texpack/weapon_atlas.py [--preview]

Needs Pillow and NumPy. The layout is the one `src/weapons/atlas.rs` expects: the image is two
halves, one above the other. Each half is a grid of 5 columns by 2 rows, and a finish's tile is
its place in `Finish::ALL`. The top half is a Weapon as it comes; the bottom half is the same but
with the shared purple camo in place of gunmetal, blued steel and worn wood, so an Upgraded
Weapon's material only shifts its UVs down half the image.

Finishes are CC0 Poly Haven sets (1K, cropped to a tile, some recoloured) and one procedural camo
made here. Source images are never committed. `--preview` also writes `atlas_preview.png`.
"""

import datetime
import io
import json
import pathlib
import subprocess
import sys
import tempfile
import urllib.request

import numpy as np
from PIL import Image, ImageFilter

ROOT = pathlib.Path(__file__).resolve().parents[2]
ASSETS = ROOT / "assets"
MANIFEST = ASSETS / "manifest.json"
API = "https://api.polyhaven.com"
SIZE = 1024
COLUMNS, ROWS = 5, 2
# The finishes in the order of `Finish::ALL`: (name, Poly Haven set, colour multiplier, roughness
# gain or None, metal: 1.0 forces metal, anything lower scales the set's own).
FINISHES = [
    ("gunmetal", "metal_plate", (0.75, 0.78, 0.85), None, 1.0),
    ("blued_steel", "metal_plate_02", (0.2, 0.24, 0.34), None, 1.0),
    ("wood", "wood_table_worn", (1.0, 0.8, 0.62), None, 0.0),
    ("olive_paint", "green_metal_rust", (0.65, 0.75, 0.45), None, 0.2),
    ("brass", "metal_plate", (1.7, 1.3, 0.5), 0.5, 1.0),
    ("tape", "hessian_230", (0.9, 0.8, 0.55), None, 0.0),
    ("glove", "brown_leather", (0.85, 0.7, 0.6), None, 0.0),
    ("sleeve", "poly_wool_herringbone", (0.55, 0.65, 0.4), None, 0.0),
    ("plain", None, None, None, None),
]


def get(url):
    request = urllib.request.Request(url, headers={"User-Agent": "zombies-texpack/1"})
    with urllib.request.urlopen(request) as response:
        return response.read()


def fetch(asset_id):
    files = json.loads(get(f"{API}/files/{asset_id}"))
    maps = {}
    for key, name in (("Diffuse", "color"), ("nor_gl", "normal"), ("arm", "orm")):
        if key == "Diffuse" and key not in files:
            key = next(k for k in ("col_1", "col_01") if k in files)
        url = files[key]["1k"]["png"]["url"]
        maps[name] = Image.open(io.BytesIO(get(url))).convert("RGB")
    return maps


def tile_box(index):
    column, row = index % COLUMNS, index // COLUMNS
    left, right = column * SIZE // COLUMNS, (column + 1) * SIZE // COLUMNS
    top, bottom = row * (SIZE // 2) // ROWS, (row + 1) * (SIZE // 2) // ROWS
    return left, top, right, bottom


def crop_to(image, box):
    width, height = box[2] - box[0], box[3] - box[1]
    crop_w = round(image.width * width / height) if width < height else image.width
    crop_h = image.height if width < height else round(image.width * height / width)
    left, top = (image.width - crop_w) // 2, (image.height - crop_h) // 2
    return image.crop((left, top, left + crop_w, top + crop_h)).resize((width, height), Image.LANCZOS)


def camo(box, seed):
    """Purple blotch camo: three nested noise levels in four purples."""
    width, height = box[2] - box[0], box[3] - box[1]
    rng = np.random.default_rng(seed)

    def noise(cells):
        small = Image.fromarray((rng.random((cells, cells)) * 255).astype(np.uint8))
        big = small.resize((width, height), Image.BICUBIC).filter(ImageFilter.GaussianBlur(2))
        return np.asarray(big, dtype=np.float32) / 255

    field = 0.55 * noise(4) + 0.3 * noise(9) + 0.15 * noise(20)
    shades = np.array([(26, 10, 42), (74, 28, 128), (122, 64, 190), (18, 6, 28)], dtype=np.float32)
    level = np.digitize(field, np.quantile(field, [0.3, 0.62, 0.86]))
    return Image.fromarray(shades[level].astype(np.uint8))


def multiply(image, factor):
    scaled = np.asarray(image, dtype=np.float32) * np.array(factor)
    return Image.fromarray(np.clip(scaled, 0, 255).astype(np.uint8))


def build():
    layers = {name: Image.new("RGB", (SIZE, SIZE)) for name in ("color", "normal", "orm")}
    sources = {}
    sets = []
    for index, (_, source, colour, rough, metal) in enumerate(FINISHES):
        box = tile_box(index)
        width, height = box[2] - box[0], box[3] - box[1]
        if source is None:
            # Flat white, a flat normal, today's roughness 0.6 and metal 0.15.
            tiles = {
                "color": Image.new("RGB", (width, height), (255, 255, 255)),
                "normal": Image.new("RGB", (width, height), (128, 128, 255)),
                "orm": Image.new("RGB", (width, height), (255, 153, 38)),
            }
        else:
            if source not in sources:
                sources[source] = fetch(source)
                sets.append(source)
            tiles = {key: crop_to(image, box) for key, image in sources[source].items()}
            tiles["color"] = multiply(tiles["color"], colour)
            orm = np.asarray(tiles["orm"], dtype=np.float32)
            if rough is not None:
                orm[..., 1] = np.clip(orm[..., 1] * rough + 40, 0, 255)
            orm[..., 2] = np.maximum(orm[..., 2], 235) if metal >= 1.0 else orm[..., 2] * metal
            tiles["orm"] = Image.fromarray(orm.astype(np.uint8))
        for key, tile in tiles.items():
            layers[key].paste(tile, (box[0], box[1]))
            # The upgraded half starts as a copy.
            layers[key].paste(tile, (box[0], box[1] + SIZE // 2))
    for index in range(3):
        box = tile_box(index)
        lower = (box[0], box[1] + SIZE // 2)
        shape = (box[2] - box[0], box[3] - box[1])
        layers["color"].paste(camo(box, 7 + index), lower)
        layers["normal"].paste(Image.new("RGB", shape, (128, 128, 255)), lower)
        layers["orm"].paste(Image.new("RGB", shape, (255, 140, 70)), lower)
    return layers, sets


def main():
    layers, sets = build()
    out = ASSETS / "weapons" / "atlas"
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        for key, image in layers.items():
            image.save(tmp / f"{key}.png")
        subprocess.run(
            [
                "cargo", "run", "--quiet", "--release", "-p", "texpack", "--",
                "--color", str(tmp / "color.png"),
                "--normal", str(tmp / "normal.png"),
                "--orm", str(tmp / "orm.png"),
                "--out", str(out),
            ],
            cwd=ROOT,
            check=True,
        )
    manifest = json.loads(MANIFEST.read_text())
    manifest["assets"] = [e for e in manifest["assets"] if e["path"] != "weapons/atlas"]
    manifest["assets"].append({
        "path": "weapons/atlas",
        "name": "Weapon atlas",
        "source": "tools/texpack/weapon_atlas.py, from " + ", ".join(f"https://polyhaven.com/a/{s}" for s in sets),
        "licence": "CC0 (Poly Haven sets, recoloured and cropped; the camo is made in this repo)",
        "fetched": datetime.date.today().isoformat(),
        "notes": "1K atlas of the gun finishes, gloves and sleeves, plus the shared purple camo; layout in src/weapons/atlas.rs",
    })
    manifest["assets"].sort(key=lambda entry: entry["path"])
    MANIFEST.write_text(json.dumps(manifest, indent=2) + "\n")
    if "--preview" in sys.argv:
        layers["color"].save(ROOT / "atlas_preview.png")


if __name__ == "__main__":
    main()
