# Which CC0 models fit each PropKind?

Research for [#298](https://github.com/jchevertonwynne/zombies/issues/298) under map [#272](https://github.com/jchevertonwynne/zombies/issues/272). It builds on [#274](https://github.com/jchevertonwynne/zombies/issues/274) (`research/cc0-asset-sources.md` on branch `research/cc0-asset-sources`), which vetted the libraries. It does not re-check licences. Checked 2026-11-13 against the Poly Haven API, the ambientCG API, the Sketchfab API and Kenney's pack pages.

## Answer in one screen

| PropKind | Best CC0 candidate | Polycount | glTF at 1k | Fit to the Prop's `size` | Verdict |
|---|---|---|---|---|---|
| Crate | Poly Haven `wooden_crate_02` | 5,176 | 2.2 MB | Stack 2 wide × 2–3 high in Blender to make a 1 m cube | **Model (kitbash a stack)** |
| Table | `painted_wooden_table` (Dining Room), `metal_office_desk` (workbenches, Offices), `wooden_table_02` (Village) | 600 / 6,898 / 196 | 1.8 / 1.6 / 0.5 MB | Two or more end to end. The 0.74 m tables squash the painted table to 0.77 height | **Model** |
| Bench | none that fits. `painted_wooden_bench` is a 0.89 m settle with arms | 630 | 2.0 MB | Height would scale by 0.56 | **Textured box** (planks on today's seat, legs and rail) |
| Counter | `vintage_wooden_drawer_01` | 5,184 | 2.1 MB | Two side by side fill 3.6 × 1.05 × 0.8 at about 1.0 / 0.96 / 0.79 | **Model** (keep the code's counter-top slab) |
| Altar | none. The nearest is `dining_table`, which has a gingham cloth | 1,048 | 2.3 MB | 0.98 / 1.14 / 0.72 | **Textured box** (stone plus an ambientCG Fabric cloth) |
| Pillar | none in any library | — | — | — | **Textured box** (architecture, like the walls) |
| Shelf | `wooden_bookshelf_worn` (library), `painted_wooden_cabinet_02` (house), `worn_metal_rack` (Quarry) | 10,106 / 966 / 6,372 | 2.8 / 1.6 / 2.0 MB | Two or three side by side, within about 10% | **Model** |
| Rubble | none. Poly Haven rocks are 100k+ polys of desert sandstone, with no masonry | — | — | — | **Textured box** (Bricks chunks plus ambientCG `Ground107` brick debris) |
| Sandbags | none. `cement_bag` is the right shape (0.70 × 0.18 × 0.46) but has modern branding | 844 | 1.4 MB | Courses of 0.18 m match the code's 0.21 m courses | **Kitbash**: stack `cement_bag` and retexture it in burlap |
| Barrel | `barrel_03` (blue steel), `Barrel_01` (rusted red), `wine_barrel_01` (Village) | 1,473 / 2,682 / 10,820 | 0.6 / 0.7 / 0.9 MB | `barrel_03` scales by 1.02 / 1.02 / 1.03, almost exact | **Model, best fit of all** |
| Machine | none. Nothing like a transformer, a 7 m generator, an excavator, a crusher, a winding drum, an ore tub, a wagon, millstones, a forge, an anvil or an oven | — | — | — | **Textured box**, dressed with small Poly Haven props |
| Tree | none usable. Poly Haven trees run from 1.7M to 17M polys, and `tree_small_02` alone is 101 MB at 1k | — | — | — | **Textured box**: an ambientCG Bark trunk plus an alpha-card crown (Foliage / LeafSet atlas) |
| Headstone | Sketchfab CC0 photogrammetry scans by Cavan3D ("Drumgoon Hill – Old Graveyard Headstone 537 / 488") | 56,006 / 64,094 faces | not exposed without login | Unknown until downloaded. They need decimating to about 1–2k and a normal-map bake | **Model, with work**. Fall back to a textured box |

**Credible models:** Crate, Table, Counter, Shelf and Barrel. Headstone and Sandbags are credible only with Blender work.
**Textured box only:** Bench, Altar, Pillar, Rubble, Machine and Tree.

Every recommended Poly Haven model together is about 18 MB of glTF at 1k (JPG textures), before KTX2 conversion. That is well inside the 300 MB budget.

## How a model must fit a Prop

From `src/maps/map/mod.rs` and `src/maps/map/build.rs`:

- `Prop { kind, zone, at, size }`. Here `at` is the **middle of its foot** on the floor it stands on. `size` is width (x), height (y) and depth (z) in metres.
- The collider is the whole `size` box (`Collider::cuboid`), centred at `at + Y * size.y / 2`. A `Barrel` uses `Collider::cylinder(size.x / 2, size.y)` instead. The doc comment says a Prop is "solid through its whole box".
- So a model has to **fill that box**. Its pivot is the bottom centre. Nothing may stick out past the collider, or the Player will clip into the visible mesh. It also must not fall far short, or the Player will be stopped by thin air. The `Tree` crown is the one exception: today it is drawn above `size.y` and isn't solid ("out of reach").
- Props are axis-aligned (`Quat::IDENTITY`). A model may be turned 90° about Y so that its long axis follows the Prop's longer of x and z.
- `nth` varies the look between Props of one kind, through a colour, a lean or a turn. A model swap can keep that by choosing a variant (for example `barrel_03` or `Barrel_01`) by `nth`.

**Fit rule used below.** Poly Haven's `dimensions` are in millimetres, Blender Z-up `[x, y, z]`. That maps to the glTF/Bevy box (x, height, depth) as `(d[0], d[2], d[1])`. For each Prop size, the model is laid N times along the long axis, then scaled per axis. Scales within about ±15% read fine. Larger non-uniform scales stretch the texture, so use copies or a Blender kitbash instead. The tables of scale factors below were computed from the API's `dimensions`.

Prop sizes come from `src/maps/map/{greybox,village,quarry}.rs` on main at `1aee470`.

## Per PropKind

All Poly Haven figures come from `https://api.polyhaven.com/assets?type=models` (fields `polycount`, `dimensions`, `max_resolution`) and `https://api.polyhaven.com/files/<id>` (glTF bundle sizes). Each model ships as glTF, FBX, USD and .blend. The glTF has 3 maps (diffuse, normal, ARM) from 1k up to `max_resolution`. "glTF 1k/2k" below means the `.gltf`, the `.bin` and the textures together.

### Crate: model, as a kitbashed stack

Sizes in use: 1.0×0.7×1.0, 0.9×0.7×0.9, 1.0×0.8×1.0, 1.2×0.7×1.2, 1.2×1.1×1.2, 1.2×1.5×1.2, 1.1×1.6×1.1. All are near-cubes, 13 of them.

| Candidate | Polys | Box (w×h×d m) | Max tex | glTF 1k / 2k | Notes |
|---|---|---|---|---|---|
| `wooden_crate_02` | 5,176 | 1.17×0.46×0.53 | 8k | 2.2 / 8.2 MB | Worn planks, rope handles. **Pick** |
| `wooden_military_crate` | 22,986 | 1.24×0.46×0.52 | 4k | 3.0 / 10.2 MB | Stencilled, hinged lid. Period-right, but 4× the polys |
| `old_military_crate` | 10,476 | 1.82×0.30×0.98 | 8k | 2.5 / 9.3 MB | Flat and wide, with a canvas flap. Better as a Box (#274's kitbash) than a Prop |
| `wooden_crate_01` | 6,576 | 0.83×0.35×0.41 | 8k | 2.3 / 8.3 MB | A small chest |

- Every Poly Haven crate is a long, low chest. On its own, one would need ×1.5–3.2 height and ×1.7–2.3 depth. **Don't scale it. Stack it.**
- Two `wooden_crate_02` side by side give 1.17×0.46×1.06. Two layers make about 1.0×0.8×1.0 at a uniform ~0.86 scale. Three layers make about 1.2×1.4×1.2. That is 4–6 crates, or 21–31k polys, per stack. Decimate in Blender to about 1.5k per crate, or bake the stack into one mesh, before multiplying by 13.
- Alternative: today's box-and-battens look with an ambientCG Planks material is a cheap stand-in for small crates.

### Table: model

Sizes: 5.0×0.74×1.0 (Dining Room ×4), 4.0×0.9×1.0 and 7.0×0.9×1.4 (workbenches), 1.2×0.74×2.8, 4.0×0.76×1.4 (plan table), 2.4×0.9×1.0, 1.4×0.76×1.4, 3.0×0.76×0.8 (Village ×4).

| Candidate | Polys | Box | Max tex | glTF 1k / 2k | Fit |
|---|---|---|---|---|---|
| `painted_wooden_table` | 600 | 2.41×0.96×1.14 | 4k | 1.8 / 7.4 MB | 5.0×0.74×1.0: ×2, scale 1.04 / **0.77** / 0.88. The height squash is visible on the legs |
| `metal_office_desk` | 6,898 | 2.00×0.79×0.95 | 8k | 1.6 / 5.2 MB | 4.0×0.9×1.0: ×2, scale 1.00 / 1.14 / 1.06. Good for workbenches and the Offices |
| `wooden_table_02` | 196 | 1.13×0.80×0.71 | 4k | 0.5 / 1.9 MB | 3.0×0.76×0.8: ×3, scale 0.88 / 0.95 / 1.13. Good for the Village |
| `dining_table` | 1,048 | 2.26×0.88×1.39 | 8k | 2.3 / 9.0 MB | Cloth-covered (gingham). Too homely for the Dining Room |
| `WoodenTable_01` | 952 | 1.80×0.55×0.66 | 4k | — | Low, a coffee table |

- The Dining Room's long tables suit `painted_wooden_table` ×2. Alternatively, raise the Prop height to ~0.9 m to avoid the squash. That would be a design call, not this ticket's.
- The square 1.4×1.4 Village tables have no good match: `round_wooden_table_02` is 0.80 m across. Use `wooden_table_02` ×2 side by side, or a textured box.

### Bench: textured box

Sizes: pews 0.6×0.5×3.0 (×6), 2.6×0.5×0.5, 0.5×0.5×2.4.

- `painted_wooden_bench` (630 polys, 1.16×0.89×0.50, 4k, glTF 2.0 MB at 1k) is the only bench-like Poly Haven model, and it is a **settle with arms and a back**. Fitting the 0.5 m collider would scale its height by 0.56. If kept at 0.89 m, its back would stand above the collider. `modular_street_seating` is contemporary and urban. No CC0 church pew turned up (Sketchfab CC0 "church pew" returned nothing).
- **Verdict:** keep today's seat-legs-rail build (`build.rs`, `PropKind::Table | PropKind::Bench`) and give it the Zone's planks material. It already reads as a bench.

### Counter: model

Sizes: 3.6×1.05×0.8 (Start Room desk), 2.6×1.0×1.0 (market stalls ×2), 0.8×1.1×5.0 (bar).

| Candidate | Polys | Box | Max tex | glTF 1k / 2k | Fit |
|---|---|---|---|---|---|
| `vintage_wooden_drawer_01` | 5,184 | 1.72×1.09×1.01 | 8k | 2.1 / 6.9 MB | 3.6×1.05×0.8: ×2 at 1.05 / 0.96 / 0.79. 5.0×1.1×0.8: ×3 at 0.97 / 1.01 / 0.79. **Pick** |
| `WoodenTable_03` (sideboard) | 2,298 | 1.33×0.83×0.56 | 4k | 0.6 / 1.9 MB | Too low (h ×1.2–1.33) |
| `painted_wooden_cabinet` | 2,227 | 1.19×1.18×0.62 | 4k | 1.9 / 6.8 MB | Farmhouse. OK for the Village bar |

- Keep the code's separate counter-top slab (`COUNTER_TOP`) over the model to hide the seams between copies.

### Altar: textured box

Sizes: 1.0×1.0×2.2, 0.9×1.0×2.4.

- No altar, plinth or communion table exists on Poly Haven, ambientCG or Sketchfab CC0 (searched). `dining_table` comes closest in shape, at 0.98 / 1.14 / 0.72, but its gingham cloth is wrong for a chapel and would need retexturing.
- **Verdict:** a stone textured box (ambientCG Marble/Travertine, or Poly Haven plaster-concrete) with an ambientCG Fabric cloth on the existing cloth pieces. Since it is a one-off per Map, it is a candidate for hand-modelling later.

### Pillar: textured box

Sizes: 0.8×4.0×0.8 (chapel), 0.4×3.8×0.4 (Quarry).

- There is no column or pillar model on Poly Haven or ambientCG. The Sketchfab CC0 "pillar" hits are 200k–1.3M-face scans of standing stones and museum reliefs.
- **Verdict:** a pillar is architecture. Treat it like the walls (#275: baked box UVs plus the Zone material and trim). Nothing is lost.

### Shelf: model

Sizes: 0.6 deep throughout. 3.0–3.4×2.2–2.4 (×8), 4.2×2.4, 2.0×2.2 (×2), 1.6×1.8.

| Candidate | Polys | Box | Max tex | glTF 1k / 2k | Fit |
|---|---|---|---|---|---|
| `wooden_bookshelf_worn` | 10,106 | 1.37×2.06×0.58 | 4k | 2.8 / 10.3 MB | 3.0×2.4: ×2 at 1.09 / 1.16 / 1.03. Library (West Hall). **Pick** |
| `painted_wooden_cabinet_02` | 966 | 1.00×2.57×0.73 | 4k | 1.6 / 6.2 MB | 3.0×2.4: ×3 at 1.00 / 0.93 / 0.82. Cheapest, house Zones |
| `worn_metal_rack` | 6,372 | 0.92×1.90×0.60 | 8k | 2.0 / 6.8 MB | 3.0×2.2: ×3 at 1.09 / 1.16 / 1.00. Quarry and Workshop |
| `Shelf_01` | 182 | 1.00×2.08×0.26 | 4k | 0.6 / 2.5 MB | Only 0.26 deep against a 0.6 collider. **No**, unless the Prop gets shallower |
| `steel_frame_shelves_01/02/03` | 4,348–8,396 | — | — | — | "clean, modern". Wrong period |

- Books to fill the library shelves: `book_encyclopedia_set_01` is 67,306 polys. Too heavy to repeat. Bake the books into the shelf texture or decimate them hard.
- Filing cabinets in the Offices: `vintage_wooden_drawer_01` (see Counter).

### Rubble: textured box

Sizes: 1.5×0.85×1.6, 1.0×0.7×1.8, 1.0×0.8×2.0, 1.4×0.7×1.2.

- Poly Haven has no masonry rubble. Its rocks are photoscanned desert or coastal stone at 100k–2.4M polys (`namaqualand_boulder_05`: 117,622 polys, 1.36×0.54×0.75, 4.7 MB at 1k). Sketchfab CC0 "rubble" offers only a flat-shaded low-poly "Dungeon Rubble" by Avramania (764 faces), which is stylised and out.
- **Verdict:** keep today's heap-plus-leaning-chunks build, textured with the Zone's Bricks or Concrete. ambientCG `Ground107` and `Ground111` are ground materials tagged *brick, debris, fragments*, which fits the heap. In the Quarry, a decimated `namaqualand_boulder_05` (to ~2k, with a normal bake) is a credible "rubble at the foot of the walls".

### Sandbags: kitbash from `cement_bag`

Sizes: 3.2×1.05×0.8, 0.8×1.05×2.8, 2.0×0.6×0.8.

- No sandbag model exists in any of the four sources (Sketchfab CC0 "sandbag(s)" returned nothing).
- `cement_bag` (844 polys, 0.70×0.18×0.46, 4k, 1.4 MB at 1k) is a filled paper sack. Its 0.18 m height matches the code's ~0.21 m courses. Five courses of 4–5 bags × 2 deep make a 3.2×1.05×0.8 wall, about 40–46 bags or ~35–39k polys. Bake each wall into one mesh in Blender.
- Its texture carries a modern red "CEMENT" brand (checked against the thumbnail), so **retexture** with an ambientCG Fabric (burlap-like) material. The flour sacks in the Bakery and the sacks in the Mill can reuse the same bag.

### Barrel: model, the best fit

Sizes: 0.65×0.95×0.65 (×17), 0.7×0.95×0.7 (×2). The collider is a cylinder.

| Candidate | Polys | Box | Max tex | glTF 1k / 2k | Fit to 0.65×0.95 |
|---|---|---|---|---|---|
| `barrel_03` | 1,473 | 0.63×0.93×0.64 | 4k | 0.6 / 2.1 MB | 1.02 / 1.02 / 1.03. Weathered blue steel drum. **Pick** |
| `Barrel_01` | 2,682 | 0.56×0.88×0.56 | 8k | 0.7 / 2.4 MB | 1.15 / 1.08 / 1.15. Rusted red drum with a hazard mark. Second variant by `nth` |
| `wine_barrel_01` | 10,820 | 0.74×0.87×0.76 | 8k | 0.9 / 2.2 MB | 0.88 / 1.09 / 0.86. Wooden. Suits the Village |
| `barrel_stove` | 11,404 | 0.60×0.86×0.60 | 8k | — | Rusted drum with fire holes. Atmospheric dressing |
| `Barrel_02` | 2,688 | — | — | — | Plastic. Wrong period |

### Machine: textured box

Instances, from the comments in each Map file:

- Greybox: transformers 2.6×2.2×1.2 (×2), the generator 7.0×2.4×2.4.
- Quarry: excavator 4.0×2.4×3.0, ore tubs 2.4×1.6×1.4 and 1.4×1.2×2.0, crusher 5.0×2.6×4.0, winding drum 5.0×2.4×3.0, wagons 3.5×1.5×2.0 (×2).
- Village: carts 3.0×1.4×1.4 (×2), millstones 3.0×1.3×3.0, forge 2.0×1.6×2.4, anvil 0.6×0.8×1.0, oven 2.0×2.0×3.0.

Nothing on Poly Haven, ambientCG or Sketchfab CC0 matches any of these at a usable polycount.

- The nearest Poly Haven machines are small: `portable_generator` 0.82×0.58×0.56, `utility_box_02` 0.92×1.12×0.43, `old_military_compressor` 0.60×1.18×1.68 (79k polys), `industrial_storage_cart` 1.60×1.38×1.10. `scandinavian_masonry_heater` (1.17×2.36×1.34) is a cylindrical stove, not an oven.
- Sketchfab CC0 "anvil" offers only Avramania's flat-shaded low-poly one. "transformer", "mine cart" and "ore cart" returned nothing relevant.

**Verdict:** a textured box (ambientCG PaintedMetal, Metal, DiamondPlate or Rust, plus the Zone trim band the code already draws), **dressed** with small Poly Haven props set against or on it: `power_box_01` (21k), `utility_box_01/02`, `modular_electric_cables`, `modular_industrial_pipes_01`, `portable_generator`, `industrial_wall_lamp`. Because the generator and the transformers are the Greybox's set pieces, they are candidates for a later Blender kitbash. The ore tubs and wagons could be kitbashed from `industrial_storage_cart`'s wheels and a rusty-metal box.

### Tree: textured box

Size: 0.5×3.2×0.5 trunk (×7, the Village Orchard). The crown is drawn overhead and isn't solid.

- Poly Haven trees are unusable at runtime: `tree_small_02` has 4,652,585 polys and is 101 MB at 1k, `jacaranda_tree` 312k, `pine_tree_01` 17.4M. The rest are saplings or desert quiver trees. ambientCG's 3D models (34 of them, via `https://ambientcg.com/api/v2/full_json?type=3d-model`) are food plus one stump (`3DTreeStump001`). Sketchfab CC0 oak scans run 278k–652k faces.
- Kenney Nature Kit (CC0, 330 files, <https://kenney.nl/assets/nature-kit>) has game-ready trees, but they are flat-coloured and stylised, which #274 rules out next to the realistic set.
- **Verdict:** keep the box trunk with ambientCG Bark (`Bark014`/`Bark016`), and replace the two crown boxes with crossed alpha cards from ambientCG's Foliage or LeafSet atlases. The crown is out of reach and isn't solid, so cards cost nothing in collision. Poly Haven `tree_stump_01` (41k polys) could be decimated for set-dressing.

### Headstone: model, with work

Size: 0.8×0.9×0.25 (`HEADSTONE` in `village.rs`), Graveyard.

- Poly Haven and ambientCG have no headstone. Sketchfab CC0 (`license=cc0&downloadable=true`) has many photogrammetry scans of real Irish graveyards. All are CC0 on their own pages, so each pick needs its page recorded per #280's manifest rule:
  - "Drumgoon Hill – Old Graveyard Headstone 537", Cavan3D, 56,006 faces: <https://sketchfab.com/3d-models/drumgoon-hill-old-graveyard-headstone-537-ae0f3420458a4ecab0d4bcf3a69eab9c>
  - "Drumgoon Hill – Old Graveyard Headstone 488", Cavan3D, 64,094 faces: <https://sketchfab.com/3d-models/drumgoon-hill-old-graveyard-headstone-488-4d1d0606e7e44a84ac532d02f35d9919>
  - "Drumgoon Hill – Old Graveyard Headstone 474 / 597", Cavan3D, 115k / 238k faces. "Abandoned Headstone – Kilbeg", galway3d, 175k faces.
- These are weathered, period-appropriate and exactly the WaW look. But they are scans: they need decimating to about 1–2k triangles, a normal-map bake, and a texture downscale. The API doesn't expose texture size, real dimensions or format without a login (`archives` came back empty), so check those on download. Expect to scale each scan to 0.8×0.9×0.25, since scans come at real-world size but not at this size.
- Fallbacks: Kenney Graveyard Kit (CC0, 90 files, <https://kenney.nl/assets/graveyard-kit>) is stylised. Avramania "Tombstone – Low Poly Medieval" (304 faces) is flat-shaded. Neither fits the look. Otherwise, today's slab-and-cap build with ambientCG stone and a moss overlay.

## Gaps (textured box only)

1. **Bench**: no pew or plain bench at the 0.5 m height.
2. **Altar**: nothing exists.
3. **Pillar**: nothing exists, and it is architecture anyway.
4. **Rubble**: no masonry rubble. Decimated boulders work in the Quarry only.
5. **Machine**: none of its 16 instances (transformers, generator, excavator, crusher, winding drum, ore tubs, wagons, carts, millstones, forge, anvil, oven) has a model. It needs dressing now and a Blender kitbash later for set pieces.
6. **Tree**: realistic CC0 trees are 0.3–17M polys. Use cards.

## Sources

- Prop semantics: `src/maps/map/mod.rs` (`PropKind`, `Prop`, `prop()`), `src/maps/map/build.rs` (`Builder::prop`). Sizes come from `src/maps/map/greybox.rs`, `village.rs` and `quarry.rs`.
- Poly Haven: `https://api.polyhaven.com/assets?type=models` (522 models on 2026-11-13) and `https://api.polyhaven.com/files/<id>` for every candidate named above. Licence per #274: <https://polyhaven.com/license>.
- ambientCG: `https://ambientcg.com/api/v2/full_json?type=3d-model` (34 models). Materials named above (`Ground107`, `Ground111`, `Bark014`, `Bark016`) come from `https://ambientcg.com/api/v2/full_json`.
- Sketchfab: `https://api.sketchfab.com/v3/search?type=models&license=cc0&downloadable=true&q=<term>` for gravestone, headstone, tombstone, sandbag(s), stone column, pillar, rubble, debris, anvil, millstone, dead tree, oak tree, transformer, mine cart, ore cart, church pew and wooden crate. Also `https://api.sketchfab.com/v3/models/<uid>` for the headstones.
- Kenney: <https://kenney.nl/assets/graveyard-kit>, <https://kenney.nl/assets/nature-kit>, <https://kenney.nl/assets/furniture-kit>, <https://kenney.nl/assets/survival-kit>. Each states "License Creative Commons CC0".
- The polycount is the Poly Haven API's `polycount` field as reported. Whether it counts triangles or quads was not checked. Sketchfab figures are its `faceCount`.
