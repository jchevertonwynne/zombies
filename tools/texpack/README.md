# texpack

Turns CC0 texture maps into the KTX2 files the game loads, and records where each set came from.

Each material is three files under `assets/materials/<id>/`, each with a full mip chain and zstd
supercompression:

| file | format | holds |
|---|---|---|
| `color.ktx2` | BC7, sRGB | base colour |
| `normal.ktx2` | BC5, linear | normal, OpenGL convention (the game rebuilds blue) |
| `orm.ktx2` | BC7, linear | occlusion (R), roughness (G), metallic (B) |

A 1K set comes to about 3.3 MB. Sources may be 8- or 16-bit PNG or JPG; 16-bit is brought down to
8-bit before encoding. Source images are never committed.

## Install

Nothing to install beyond Rust. `texpack` is a workspace crate built on the
[`ctt`](https://github.com/cwfitzgerald/ctt) library, whose BC7 and BC5 encoders come as prebuilt
kernels. The first build compiles them (about a minute).

## A Poly Haven set

```
python3 tools/texpack/polyhaven.py dark_wooden_planks damaged_plaster
```

For each id this downloads the 1K `Diffuse`, `nor_gl` and `arm` PNGs into a temporary directory,
runs `texpack`, writes `assets/materials/<id>/`, and adds or replaces the set's entry in
`assets/manifest.json`. It prints each set's real-world size, which is the tile size its
`Material` takes in `src/maps/map/textures.rs`. Use `--res 2k` for a hero floor.

## An ambientCG set

```
python3 -I tools/texpack/ambientcg.py PaintedPlaster003 --size 2
python3 -I tools/texpack/ambientcg.py Tiles133D --tiles 12 --tile-m 0.15
python3 -I tools/texpack/ambientcg.py Sign004 --decal
```

For each id this asks the ambientCG API for the asset, downloads its `1K-PNG` zip into a new empty
temporary directory, reads only the maps it needs by exact member name (`Color`, `NormalGL`,
`AmbientOcclusion`, `Roughness`, `Metalness`; nothing is extracted by path or run), has `texpack` pack
the three greyscale maps into the ORM (`--ao`, `--roughness`, `--metalness`), and writes
`assets/materials/<lowercase id>/` with its manifest entry. NormalGL is the OpenGL-convention normal
the game wants (never `NormalDX`). The tile size is the API's `dimensionX` where it gives one; most
photo scans don't, so give it as `--size <metres>`, or measure it from the image: `--tiles` is how
many tiles fit across one repeat and `--tile-m` one tile's width. With `--decal` it makes
`assets/decals/<lowercase id>/color.ktx2` from `Color` and `Opacity` instead (a 1K sign is about
0.4 MB). The manifest `source` is the asset URL and the licence is "CC0"; ambientCG's asset pages
carry no licence label, so the notes cite the [site licence](https://docs.ambientcg.com/license/) as
the evidence.

## A decal

A decal is one file, `assets/decals/<id>/color.ktx2`: BC7 sRGB with the opacity as alpha.

```
cargo run --release -p texpack -- --color Color.png --opacity Opacity.png --gain 3.5 --out assets/decals/<id>
cargo run --release -p texpack -- --stain 1 --out assets/decals/oil_stain_1
```

`--gain` scales the opacity, for decals that are too faint to read in game (the ambientCG leaks
take 3.5). `--stain <seed>` makes a procedural oil stain here, so it's CC0 with
`tools/texpack --stain <seed>` as its source. Add each decal to the manifest by hand.

## A sky

A sky is one file, `assets/skies/<id>/sky.ktx2`: an equirectangular HDRI as a cubemap of six BC6H
faces with a full mip chain and zstd supercompression (512 px faces come to about 1.7 MB). The
camera shows it as its `Skybox`. The tool also writes the sky prefiltered for lighting, beside it:
`diffuse.ktx2` (a 32 px cosine-convolved irradiance cubemap, a few KB) and `specular.ktx2` (a
GGX-filtered chain, mip `m` of `n` being perceptual roughness `m / (n - 1)`, about 1.7 MB), which
the camera uses as a plain `EnvironmentMapLight`. Bevy's `GeneratedEnvironmentMapLight` would filter
the sky on the GPU every frame, which cost the Greybox about 4 ms.

```
cargo run --release -p texpack -- --sky belfast_sunset_puresky_2k.hdr --yaw 115.7 --out assets/skies/belfast_sunset_puresky
```

Use a 2K `.hdr` from Poly Haven. `--yaw` turns the sky about the up axis, and the tool prints the
direction of its brightest pixel (the sun), so the sun can be lined up with the Layout's `sun`.
`--size` sets the face size (default 512). Add the entry to the manifest by hand.

## The weapon atlas

`python3 tools/texpack/weapon_atlas.py` (needs Pillow and NumPy) fetches the Poly Haven sets behind the gun finishes,
packs them into one 1K image, runs `texpack` on it and writes `assets/weapons/atlas/` and its manifest entry. The
layout (a top half for a Weapon as it comes, a bottom half with the purple camo for an Upgraded one) is in
`src/weapons/atlas.rs`.

## Any other set

```
cargo run --release -p texpack -- --color diff.png --normal nor_gl.png --orm arm.png --out assets/materials/<id>
```

Pack separate AO, roughness and metal maps into one ORM image first (R, G, B). Then add an entry to
`assets/manifest.json` by hand.

## The manifest

`assets/manifest.json` lists every asset: the file or directory it covers (`path`, relative to
`assets/`), a `name`, its `source` URL, the `licence` as the source states it, the date `fetched`,
and an optional `archive` link (a Wayback capture, say) for sources whose terms are per-asset or
contradictory. `cargo test provenance` fails if any file under `assets/` isn't covered by an entry,
or if `assets/` grows past its 512 MB budget, and prints the running total.

The synthesised placeholder sounds from `tools/gen-sounds` are covered by one `audio` entry until
the Audio lane replaces them. Each real recording gets its own, more specific entry.

## Kitbashed Prop units

The Crate stacks, Sandbag courses and Headstone are built by `tools/props/build_props.py`, which
drives headless Blender and uses `texpack` for their textures. See its docstring.
