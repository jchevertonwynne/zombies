"""Makes the Walker's three colour maps: decayed skin, a stained sweater and soiled trousers,
from the official CC0 MakeHuman maps (the uniform skin, the fisherman sweater and the wool
trousers). Writes PNGs for `texpack` (see `build.sh`). Runs in Blender's Python, which has numpy:

    blender -b --python tools/walker/make_textures.py -- --packs <dir> --out <dir>

Everything made here is a function of a seed, so a rerun gives the same maps.
"""

import os
import sys

import bpy
import numpy as np


def args():
    argv = sys.argv[sys.argv.index("--") + 1 :]
    return {argv[i].lstrip("-"): argv[i + 1] for i in range(0, len(argv), 2)}


def load(path):
    image = bpy.data.images.load(path)
    image.colorspace_settings.name = "Non-Color"
    w, h = image.size
    pixels = np.empty(w * h * 4, dtype=np.float32)
    image.pixels.foreach_get(pixels)
    return pixels.reshape(h, w, 4)[:, :, :3].copy()


def save(path, rgb):
    h, w, _ = rgb.shape
    image = bpy.data.images.new(os.path.basename(path), w, h, alpha=False)
    image.colorspace_settings.name = "Non-Color"
    rgba = np.ones((h, w, 4), dtype=np.float32)
    rgba[:, :, :3] = np.clip(rgb, 0.0, 1.0)
    image.pixels.foreach_set(rgba.ravel())
    image.filepath_raw = path
    image.file_format = "PNG"
    image.save()


def upsample(grid, size):
    """Bilinear resample of a square random grid to size x size."""
    n = grid.shape[0]
    coords = np.linspace(0, n - 1, size)
    i0 = np.floor(coords).astype(int).clip(0, n - 2)
    t = coords - i0
    rows = grid[i0] * (1 - t)[:, None] + grid[i0 + 1] * t[:, None]
    return rows[:, i0] * (1 - t)[None, :] + rows[:, i0 + 1] * t[None, :]


def fbm(rng, size, octaves=(4, 8, 16, 32, 64, 128), gain=0.55):
    total = np.zeros((size, size), dtype=np.float32)
    amp, norm = 1.0, 0.0
    for n in octaves:
        total += amp * upsample(rng.random((n + 1, n + 1), dtype=np.float32), size)
        norm += amp
        amp *= gain
    return total / norm


def stained(base, rng, tint, darken, blotches):
    """`base` greyed and tinted, then mottled by `blotches`: [(colour, threshold, strength)]."""
    size = base.shape[0]
    grey = base.mean(axis=2, keepdims=True)
    out = (0.4 * base + 0.6 * grey) * np.array(tint, dtype=np.float32) * darken
    for colour, threshold, strength in blotches:
        mask = np.clip((fbm(rng, size) - threshold) / 0.12, 0.0, 1.0)[:, :, None] * strength
        out = out * (1 - mask) + np.array(colour, dtype=np.float32) * mask
    grime = (0.75 + 0.5 * fbm(rng, size, octaves=(32, 64, 128, 256)))[:, :, None]
    return out * grime


def main():
    a = args()
    packs, out = a["packs"], a["out"]
    os.makedirs(out, exist_ok=True)
    rng = np.random.default_rng(346)

    skin = load(os.path.join(packs, "skins/blindsaypatten_uniform_skin_texture/skin_texture.png"))
    skin = stained(
        skin,
        rng,
        tint=(0.78, 0.9, 0.7),
        darken=0.8,
        blotches=[
            ((0.3, 0.22, 0.34), 0.56, 0.7),  # bruising
            ((0.5, 0.55, 0.26), 0.62, 0.6),  # sickly rot
            ((0.2, 0.04, 0.03), 0.68, 0.85),  # dried blood
        ],
    )
    save(os.path.join(out, "skin.png"), skin)

    sweater = load(os.path.join(packs, "clothes/toigo_fisherman_sweater/shirt-knit.png"))
    sweater = stained(
        sweater,
        rng,
        tint=(0.75, 0.72, 0.6),
        darken=0.55,
        blotches=[((0.12, 0.09, 0.06), 0.55, 0.8), ((0.3, 0.06, 0.04), 0.66, 0.8)],
    )
    save(os.path.join(out, "shirt.png"), sweater)

    trousers = load(os.path.join(packs, "clothes/toigo_wool_pants/Pants_wool.png"))
    trousers = stained(
        trousers,
        rng,
        tint=(0.7, 0.7, 0.68),
        darken=0.6,
        blotches=[((0.1, 0.08, 0.05), 0.5, 0.85), ((0.28, 0.07, 0.04), 0.67, 0.7)],
    )
    save(os.path.join(out, "pants.png"), trousers)

    # A fully opaque opacity map, for `texpack --color --opacity` (a colour-only map).
    save(os.path.join(out, "opaque.png"), np.ones((2048, 2048, 3), dtype=np.float32))


main()
