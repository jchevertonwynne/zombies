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

## The Refinery's Valve (#572)

`kitbash.py` makes the three parts of a Fuel Line's Valve: `valve_stand` (a riser to 1.1 m on a foot, a gauge case with a white face, a 2.4 m
mast of alternating yellow and black hazard bands with a cage for the lamp), `valve_wheel` (a 0.6 m red hand wheel, an 8-segment rim with four
spokes, its origin the hub) and `valve_needle` (the gauge's red needle, its origin the pivot). Each faces +z in the game (Blender's -y) and wears
`materials/rusty_metal_02`, tinted, with flat glTF colours for the paint; no new texture files. The Valve lamp is not modelled: the game draws it,
as it changes colour.

```
python3 -I tools/props/build_props.py --only valves        # all three
python3 -I tools/props/build_props.py --only valve_stand   # one
```

They come out as `assets/props/<name>/<name>.{gltf,bin}` and a manifest entry each. They are not fitted into a Prop's box: `src/maps/fire_look.rs`
stands them at each Valve (`HUB_AT`, `GAUGE_AT`, `MAST_AT` and `LAMP_AT` there are the places in the stand's frame; keep them in step with
`VALVE_*` in `kitbash.py`), turning the wheel and the needle. Headless runs, and a game without the files, draw the same parts from code.

## The Refinery's Props (#574)

Eight Poly Haven models (CC0, each page read for its licence) and five kitbashes for the Refinery's 72 Props and its six floodlight masts.

`props.py` makes the five plain models with the `decimate.py` path: `concrete_road_barrier` (the jersey barriers), `utility_box_01` (the booth's cabinet, the
yards' cabinets and the Control House's panels), `portable_welding_cart`, `old_military_compressor` and `metal_jerrycan`. The other three models are catalogues of
separate pieces, so `assemble.py` (headless Blender) takes the pieces it wants, decimates them in place, turns them and joins them into one unit each, with a
few boxes to carry them:

- `modular_industrial_pipes_01` splits into `pipe_manifold` (a straight, the red-wheeled valve, a tee and a straight on two legs and a skid, 1.7k triangles,
  two materials: the catalogue's two texture groups) and `pipe_run` (three long straights on two saddles, 2.0k triangles, which wears the manifold's first group:
  its glTF points at `../pipe_manifold/g1_*.ktx2`, so the textures are on disk once);
- `modular_fire_escape` gives `fire_escape`, its 6.5 m ladder cut to 3 m inside four hoops of boxes (a caged ladder, 0.8k triangles). Only the ladder is used:
  the stairs and the rails are not, since the Refinery's stairs and railings are built from primitives;
- `flood_lights` gives `flood_lights`, a twin pair of its wall-mounted heads on a cross bar (1.7k triangles), the head of each floodlight's mast. It faces +Z.

The ladder cage and the lamp pair are near-black or thin steel, so they wear flat glTF colours and no textures (about 7 MB of the budget saved).

```
python3 -I tools/props/props.py concrete_road_barrier utility_box_01 portable_welding_cart old_military_compressor metal_jerrycan
python3 -I tools/props/props.py pipe_manifold pipe_run fire_escape flood_lights     # build the manifold before the run
python3 -I tools/props/build_props.py --only refinery                              # the five kitbashes
python3 -I tools/props/build_props.py --only tanker                                # or one
```

`kitbash.py` makes `tanker` (a road tanker: tank, chassis, cab, wheels; `container_side`, `rusty_metal_04`, `rusty_metal_02`), `transformer` (4.6 m thick, 10 m long, laid three
times along the Substation's pen: `green_metal_rust` over a `rebar_reinforced_concrete` plinth, radiator fins, a conservator and three bushings), `pump_set`
(`blue_metal_plate` motor on a skid, volute, suction and discharge pipes), `loading_arm` (a riser, swivel, boom and drop pipe) and `flood_mast` (an 8.9 m lattice of four legs,
rungs and diagonals under a platform). They wear material sets already under `assets/materials`, so no new texture files (about 0.24 MB in all). The build prints each
unit's size: the `model(..)` and `kitbash(..)` constants in `src/maps/map/mod.rs` (and `FLOOD_HEADS` and `FLOOD_MAST`, which `src/maps/darkness/flood.rs` stands at each
flood) repeat them, so keep the two in step.

The Props that wear them are laid by `tools/refinery_layout.py` (its `PROPS` list, written into `PROPS` in `refinery/data.rs`), which also steers every path node and edge round them.
