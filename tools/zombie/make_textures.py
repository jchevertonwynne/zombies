"""Makes the Zombie bodies' colour maps: decayed skin (shared by every body), and a stained shirt and
soiled trousers for each of the three bodies (labourer, soldier, civilian), from the official CC0
MakeHuman maps. Writes PNGs for `texpack` (see `build.sh`). Runs in Blender's Python, which has
numpy:

    blender -b --python tools/zombie/make_textures.py -- --packs <dir> --out <dir>

Everything made here is a function of a seed, so a rerun gives the same maps. Each map's bottom-left
corner is painted dark red: the cap faces of a beheaded or gibbed body (see `build_bodies.py`) take
their UVs there. The soldier's and civilian's clothes are half size, to keep `assets/` small.
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


def shrink(rgb, factor):
    """`rgb` at 1/`factor` size, by averaging blocks."""
    h, w, c = rgb.shape
    return rgb.reshape(h // factor, factor, w // factor, factor, c).mean(axis=(1, 3))


def stamp(rgb):
    """Paints the cap patch into the bottom-left corner (row 0 is the bottom in Blender's pixels)."""
    size = max(8, rgb.shape[0] // 64)
    rgb[:size, :size] = (0.3, 0.035, 0.03)
    return rgb


# Each cloth map: (out name, source under packs/, factor to shrink by, tint, darken, blotches).
GRIME = [((0.1, 0.08, 0.05), 0.5, 0.85), ((0.28, 0.07, 0.04), 0.67, 0.7)]
CLOTHES = [
    ("labourer_shirt", "clothes/toigo_fisherman_sweater/shirt-knit.png", 1, (0.62, 0.7, 0.74), 0.7, GRIME),
    ("labourer_pants", "clothes/toigo_wool_pants/Pants_wool.png", 1, (0.62, 0.66, 0.72), 0.75, GRIME),
    ("soldier_shirt", "clothes/toigo_basic_tucked_t-shirt/T-shirt_basic.png", 2, (0.5, 0.56, 0.4), 0.9, GRIME),
    ("soldier_pants", "clothes/cortu_cargo_pants/cargo_pants_diff.png", 2, (0.55, 0.6, 0.42), 0.8, GRIME),
    ("civilian_shirt", "clothes/namuhekam_male_polo_shirt/Polo_Base_Color.png", 2, (0.66, 0.62, 0.56), 0.8, GRIME),
    ("civilian_pants", "clothes/cortu_jeans_shorts/jean_shorts_diff.png", 2, (0.5, 0.56, 0.68), 0.7, GRIME),
]


def main():
    a = args()
    packs, out = a["packs"], a["out"]
    os.makedirs(out, exist_ok=True)
    rng = np.random.default_rng(347)

    skin = load(os.path.join(packs, "skins/blindsaypatten_uniform_skin_texture/skin_texture.png"))
    skin = stained(
        skin,
        rng,
        tint=(0.62, 0.82, 0.74),
        darken=0.95,
        blotches=[
            ((0.34, 0.26, 0.38), 0.56, 0.65),  # bruising
            ((0.5, 0.56, 0.3), 0.62, 0.6),  # sickly rot
            ((0.26, 0.06, 0.05), 0.69, 0.7),  # dried blood
        ],
    )
    save(os.path.join(out, "skin.png"), stamp(skin))

    for name, source, factor, tint, darken, blotches in CLOTHES:
        cloth = load(os.path.join(packs, source))
        if factor > 1:
            cloth = shrink(cloth, factor)
        save(os.path.join(out, name + ".png"), stamp(stained(cloth, rng, tint=tint, darken=darken, blotches=blotches)))

    # A fully opaque opacity map, for `texpack --color --opacity` (a colour-only map), each size.
    for size in (2048, 1024):
        save(os.path.join(out, f"opaque{size}.png"), np.ones((size, size, 3), dtype=np.float32))


main()
