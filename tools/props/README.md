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

## The Archive's book rows (#537)

`kitbash.py` also makes the shelf fill of every Bookcase and fixed stack in the Archive, `book_rows`: one mesh per face of
a stack (front and top faces of the spines only, no backs or sides), 6 shelves of about 50 spines, each spine tinted per
vertex (`COLOR_0`) from a 12-colour library palette over the `book_pattern` cloth, folios on the bottom shelf. Three
variants (`a`, `b` mirrored, `c` offset) so neighbouring stacks never match; the other face of a stack is the same mesh
turned half a revolution, so all of them batch.

```
python3 -I tools/props/build_props.py --only book_rows
```

It writes `assets/props/book_rows/book_rows_{a,b,c}.{gltf,bin}` (no new texture files: they wear `materials/book_pattern`) and
one manifest entry. Unlike the other kitbashes it is not fitted into a Prop's box by `fitting`: `src/maps/stacks_look.rs` lays it
over both faces of each stack, whose own size is the one it was modelled for (1.2 x 3.0 x 5.0 m).

## The Archive's Wheels (#538)

`kitbash.py` also makes the three parts of a Range's Wheel: `wheel_post` (a 1.1 m fluted cast-iron post with a brass cap, its
origin the middle of its foot, with the axle out of its face), `aisle_wheel` (a 0.7 m three-spoke hand wheel with a brass hub, its
origin the hub, so the game can turn it) and `wheel_chain` (the chain woven through the spokes and padlocked to the post, shown
only while the Range is Sabotaged). Each faces +z in the game (Blender's -y) and wears `materials/rusty_metal_02` and
`materials/metal_plate_02`, tinted; no new texture files.

```
python3 -I tools/props/build_props.py --only wheels        # all three
python3 -I tools/props/build_props.py --only aisle_wheel   # one
```

They come out as `assets/props/<name>/<name>.{gltf,bin}` and a manifest entry each. They are not fitted into a Prop's box:
`src/maps/stacks_look.rs` stands them at each Wheel (`HUB_AT` there is the hub's place in the post's frame; keep the two in step). Headless
runs, and a game without the files, draw the same parts from code.
