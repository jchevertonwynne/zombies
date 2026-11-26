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

## The Archive's reading level (#540)

`props.py` also makes eight Poly Haven models for the Catalogue Hall, Reading Room, Map Room and Lecture Theatre: `dining_table` (the
fitting lays it twice as a 4.5 m reading table), `SchoolChair_01`, `marble_bust_01` and `book_encyclopedia_set_01` (dressing: no collider of
their own, `src/maps/map/garnish.rs`), `wooden_ladder`, `vintage_grandfather_clock_01`, `standing_chalkboard_01` and `GothicCabinet_01`.
`marble_bust_01` has only a roughness map, so `props.py` packs it with texpack's `--roughness` (unoccluded, non-metal).

```
python3 -I tools/props/props.py dining_table SchoolChair_01 wooden_ladder ...   # or no names, for all of UNITS
```

`kitbash.py` makes the eight kitbashes (`card_catalogue`, `issue_desk`, `reading_lamp`, `lectern`, `theatre_bench`, `projector`, `display_case`
and `globe`), each a mesh of at most 2k triangles in material sets already under `assets/materials` (wood, paper, leather, brass) and, for the
lamp's shade and bulb (emissive green x 3 and warm x 10, `KHR_materials_emissive_strength`), the display case's glass and the globe's ink, flat
glTF factors. There is no CC0 globe (#514): `globe` is a Paper006 sphere tinted sepia with a lat/long graticule laid on it as thin ink quads, no
continents, in a brass meridian ring on a turned stand.

```
python3 -I tools/props/build_props.py --only archive   # all eight
python3 -I tools/props/build_props.py --only globe     # one
```

They come out as `assets/props/<name>/<name>.{gltf,bin}` and a manifest entry each. Their sizes are declared by the `kitbash(..)` constants in
`src/maps/map/mod.rs` (and `garnish.rs`'s `READING_LAMP`); keep the two in step. The issue desk is an octagon 6 m across its flats, so it stays
within its Prop's 6 x 6 m box. The Lecture Theatre's seven tier benches (a side) are modelled in the one `theatre_bench` unit that fits the tiers'
single Prop, since splitting the tiers into stepped Props would make them climbable.

## The Archive's stacks and cellar (#541)

No new Poly Haven model: the ring halls' ladders, the Bindery's work benches and drying rack, the Pump Room's drums and tool rack reuse
`wooden_ladder`, `wooden_table_02`, `worn_metal_rack` and `Barrel_01`/`barrel_03`. `kitbash.py` makes nine more kitbashes in the same way as the
reading level's, from material sets already under `assets/materials` (`green_metal_rust` for the deed boxes and the pump engine): `book_trolley`,
`book_pile`, `book_press`, `guillotine`, `safe`, `deed_boxes`, `pump_engine`, `bucket` and `candles` (wax is a flat glTF factor; the flames are
not modelled, `src/maps/map/garnish.rs` draws and flickers them).

```
python3 -I tools/props/build_props.py --only archive_stacks   # all nine
python3 -I tools/props/build_props.py --only safe             # one
```

Their sizes are declared by the `kitbash(..)` constants in `src/maps/map/mod.rs` (and `CANDLES`): the build prints each unit's size, copy it there.
The Props that wear them are laid by `tools/archive_layout.py` (its `STACKS` list, which writes `STACKS_PROPS` in `archive/data.rs`).
