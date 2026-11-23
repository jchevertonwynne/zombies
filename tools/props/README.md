# props

Turns Poly Haven (CC0) models into the unit models the game fits into its Props (`src/maps/map/fitting.rs`).

```
python3 tools/props/props.py                 # every model in UNITS
python3 tools/props/props.py barrel_03 ...   # just these
```

For each id it downloads the 1K glTF and textures, runs Blender headless (`decimate.py`) to join the
meshes, decimate to at most ~2k triangles and stand the unit with its foot's middle at the origin, its
length along +X and its depth along +Z, packs the textures with `texpack`, rewrites the glTF to use the
KTX2 files, and adds the manifest entry. Output is `assets/models/<id>/{model.gltf,model.bin,color.ktx2,normal.ktx2,orm.ktx2}`.

`UNITS` in `props.py` holds each unit's size (length, height, depth). The same size is declared by the
model's `PropVariant` in `src/maps/map/mod.rs`; keep the two in step. Set `BLENDER` if Blender isn't at
`/Applications/Blender.app`.

## The Sanatorium's Props (#525)

`props.py` also makes the Sanatorium's seven Poly Haven models (`old_bed_frame`, `wheelchair_01`, `tool_cart`,
`steel_frame_shelves_01`, `drawer_cabinet`, `WetFloorSign_01`, `vintage_day_bed`). The gurney, dressing trolley,
locker bank, cot, mortuary slab, bath tub, rocking horse and derelict ambulance are kitbashes, modelled from boxes
and cylinders in headless Blender and wearing material sets already under `assets/materials`:

```
python3 -I tools/props/build_props.py --only sanatorium   # all eight
python3 -I tools/props/build_props.py --only gurney       # one
```

Each comes out as `assets/props/<name>/<name>.{gltf,bin}` (a mesh with one primitive per material set, at most 2k
triangles, its foot's middle at the origin) and a manifest entry. Their sizes are declared by the `kitbash(..)` constants
in `src/maps/map/mod.rs`; keep the two in step.
