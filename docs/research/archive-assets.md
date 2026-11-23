# The Archive: CC0 textures, models and sounds (#514, map #511)

Researched 2026-11-23. Nothing was downloaded into the repo. Every licence below was read on the
source's own page (or API record) on that date; each row says where.

## Verdict

- Poly Haven and ambientCG cover wood, stone, plaster, leather and cloth well. **Paper is only on ambientCG** (Poly Haven has no paper or cardboard material).
- Poly Haven covers books, shelves, ladders, lamps, cases, clocks and furniture. **There is no globe** on Poly Haven (name and tag search over all 522 models; the only "globe" hits are light bulbs); nothing in this list is a globe. Options: a Blender-built globe (ours, CC0) or skip it.
- Freesound (CC0 filter) covers wood creak, drips, page turn, paper rustle, hall room tone and a stone slide. **Rail rumble** has only train/tunnel/wagon recordings, not a shelf on rails; use them low-passed and pitched down. A **Stack stopping** is best built from a heavy-book or stone thud plus a wood creak.
- Budget: the Archive can be fully dressed in roughly 110 MB, well inside the headroom (see Budget).

## Licences confirmed (read on the source)

| Source | Where the licence was read | Result |
|---|---|---|
| Poly Haven | https://polyhaven.com/license ("Our assets are all licensed as CC0"); each asset's page and API record at https://api.polyhaven.com/info/<id> | CC0 for textures, models and HDRIs. The site Terms of Service (and the API terms) govern the website and API, not the assets. |
| ambientCG | https://docs.ambientcg.com/license/ ("All ambientCG assets are provided under the Creative Commons CC0 1.0 Universal License"); the asset page https://ambientcg.com/a/Paper001 repeats it | CC0. |
| Freesound | each sound's own page, which links `creativecommons.org/publicdomain/zero/1.0/`; all 27 URLs below were opened and checked individually | CC0 only for the ones listed. Many Freesound sounds are CC-BY; the search used the "Creative Commons 0" filter and each page was re-read. |
| Kenney | the asset pages show "License Creative Commons CC0"; https://kenney.nl/support says all game assets are public domain (CC0) | CC0. |
| OpenGameArt | each content page shows `License(s): CC0` | CC0 for the three entries below. OpenGameArt hosts mixed licences, so check per page. |

Existing repo practice (assets/manifest.json): 81 Poly Haven, 135 Freesound, 15 Kenney, 3 ambientCG entries. The tools already exist: `tools/texpack/polyhaven.py`, `tools/props/props.py` and `tools/audio/process.py`. ambientCG has no fetch script yet (the three Leaking decals were fetched by hand); `texpack` takes any colour, normal and ORM maps, so a small `ambientcg.py` beside `polyhaven.py` would do it.

## Budget

- `assets/` is 292 MB today (materials 209, models 35, enemies 20, skies 16, audio 3.6, props 3.6, rest under 2). The limit is 512 MB (#429), so headroom is about 220 MB.
- Measured cost per item in the repo: a 1K material set is about 3.2 MB (assets/materials/dark_wooden_planks is 3.2 MB; texpack README says about 3.3 MB); a decimated prop model is about 3.3 MB (assets/models/wooden_bookshelf_worn is 3.4 MB). Sounds are small OGGs of 20 to 200 KB each.
- Source download sizes are not the repo cost: a Poly Haven 1K set is 5.6 to 17.7 MB of PNG before packing; ambientCG 1K-JPG zips are 2.5 to 7.9 MB.
- Suggested pull: 16 material sets (about 53 MB), 16 models (about 55 MB), 25 sound files (about 3 MB), about 110 MB total, leaving about 110 MB. Cut hard on materials first (use 1K; keep 2K for at most one hero floor).
- Materials already in the repo and reusable as-is for the Archive: `dark_wooden_planks`, `weathered_planks`, `damaged_plaster`, `plastered_stone_wall`, `white_plaster_rough_01`, `slate_floor_02`, `dark_brick_wall`, `castle_brick_02_red`, `hessian_230`, `mossy_cobblestone`. Models already in the repo: `wooden_bookshelf_worn`, `worn_metal_rack`, `vintage_wooden_drawer_01`, `painted_wooden_cabinet_02`, `painted_wooden_table`, `wooden_table_02`, `metal_office_desk`.

## Material sets

Poly Haven (CC0; page `https://polyhaven.com/a/<id>`, files via `https://api.polyhaven.com/files/<id>`). "Tile" is the real-world size from the API. All have a 1K and 2K set.

| Role | Id | Tile | Notes |
|---|---|---|---|
| Wood floor | `old_wooden_floor_02` | 2.03 m | worn hardwood planks |
| Wood floor | `old_wooden_floor_01` | 2.0 m | alternate |
| Wood floor (hero) | `herringbone_parquet` | 3.4 m | reading-hall floor |
| Wood floor | `wood_floor_worn` | 2.0 m | pine, weathered |
| Wood wall/shelf | `dark_paneled_wood` | 1.4 m | lacquered panelling for the reading hall |
| Wood wall | `wooden_panels` | 2.1 m | recessed panels |
| Wood desk/shelf | `wood_table_worn` | 0.55 m | stained, damaged |
| Stone wall | `old_stone_wall_02` | 2.085 m | rough weathered |
| Stone wall (vault) | `medieval_blocks_05` | 2.0 m | smallest source (5.6 MB PNG) |
| Stone wall | `castle_wall_slates` | 2.5 m | stacked blocks |
| Stone floor | `monastery_stone_floor` | 1.8 m | slate pathway |
| Stone floor | `stone_tiles_03` | 1.92 m | worn, dirty |
| Plaster | `worn_plaster_wall` | 1.8 m | crumbling, chipped |
| Plaster | `worn_cracked_plaster` | 1.8 m | cracked, discoloured |
| Plaster (indoor) | `plastered_wall_04` | 3.2 m | rough, scratched; has the `indoor` tag |
| Plaster over stone | `plaster_stone_wall_01` | 2.3 m | stone showing through |
| Leather | `brown_leather` | 0.4 m | vintage upholstery, matte |
| Leather | `fabric_leather_01` | 0.4 m | aged couch leather |
| Leather | `fabric_leather_02` | 0.5 m | stitched, aged |
| Book cloth | `book_pattern` | 0.3 m | olive woven cotton; files are named `col1`/`col2`/`arm`/`nor_gl` rather than `Diffuse`, so `polyhaven.py` needs a small tweak |

Paper and cardboard (ambientCG only; CC0 per https://docs.ambientcg.com/license/; page `https://ambientcg.com/a/<id>`; sizes are the 1K-JPG zip from the ambientCG API, https://ambientcg.com/api/v2/full_json):

| Id | Tags | 1K-JPG zip |
|---|---|---|
| `Paper001` | white paper | 5.0 MB |
| `Paper003` | creased white paper | 2.5 MB |
| `Paper006` | beige/brown paper | 4.8 MB |
| `Paper002` | brown paper | 5.6 MB |
| `Cardboard002` | brown, clean | 6.0 MB |
| `Cardboard001` | brown, broken | 6.7 MB |

Leather alternatives on ambientCG (same licence): `Leather014` (brown, old, scratched), `Leather037` (brown, clean, dark), `Leather030` (brown, dark), `Leather028` (brown). Not needed unless Poly Haven's 0.3 to 0.5 m tiles look too repetitive.

Paper as a material is mostly for small meshes (loose sheets, book pages, crates of documents), so a 1K set is plenty.

## Models

Poly Haven (CC0; `https://polyhaven.com/a/<id>`). Size is the 1K glTF download from the API; the repo copy is about 3 MB after decimation and KTX2 packing (see `tools/props/README.md`; add each to `UNITS` in `tools/props/props.py`). Dimensions are from the API, in mm.

| Role | Id | Dimensions (mm) | 1K source | Notes |
|---|---|---|---|---|
| Book set | `book_encyclopedia_set_01` | 551 x 163 x 237 | 3.3 MB | a row of encyclopedias |
| Book set | `decorative_book_set_01` | 2505 x 321 x 286 | no `gltf` key at 1K in the API | many covers (hardcover/softcover/magazine); needs a look before use |
| Notebook | `binder_notebook` | 584 x 200 x 25 | n/a | loose item |
| Shelf (existing) | `wooden_bookshelf_worn` | 1374 x 581 x 2063 | 2.8 MB | already in the repo |
| Shelf | `Shelf_01` | 1003 x 257 x 2080 | 0.6 MB | cheap, tall |
| Shelf | `painted_wooden_shelves` | 511 x 367 x 1125 | 2.1 MB | |
| Shelf | `wooden_display_shelves_01` | 1078 x 372 x 1556 | 0.5 MB | |
| Cabinet | `vintage_cabinet_01` | 2022 x 670 x 2235 | 2.9 MB | card catalogue stand-in |
| Cabinet | `GothicCabinet_01` | 1719 x 1129 x 2361 | 1.0 MB | |
| Desk/table | `dining_table` | 2256 x 1390 x 877 | 2.3 MB | reading table |
| Desk/table | `round_wooden_table_01` | 1399 x 1399 x 1005 | 0.6 MB | |
| Chair | `WoodenChair_01` | 688 x 658 x 2274 | 1.1 MB | |
| Chair | `SchoolChair_01` | 566 x 675 x 1006 | 0.5 MB | |
| Stool | `wooden_stool_01` | 425 x 442 x 437 | 1.0 MB | |
| Ladder | `wooden_ladder` | not read | 2.6 MB | library ladder stand-in |
| Ladder | `wooden_ladder_02` | not read | 2.3 MB | |
| Ladder | `ladder_sectioned_01` | not read | 2.6 MB | |
| Lamp | `vintage_oil_lamp` | 220 x 220 x 802 | 2.1 MB | warm lamplight |
| Lamp | `Lantern_01` | 122 x 97 x 294 | 1.4 MB | |
| Lamp | `wooden_lantern_01` | 221 x 235 x 530 | 3.8 MB | |
| Lamp (desk) | `desk_lamp_arm_01` | 617 x 408 x 879 | 2.9 MB | |
| Lamp (hanging) | `Chandelier_01` | 746 x 816 x 798 | 1.3 MB | |
| Lamp (hanging) | `lantern_chandelier_01` | 579 x 575 x 878 | 1.5 MB | |
| Lamp (wall) | `industrial_wall_sconce` | 150 x 252 x 342 | 1.8 MB | |
| Case | `vintage_suitcase` | 1608 x 239 x 568 | 4.3 MB | |
| Case | `treasure_chest` | 959 x 523 x 619 | 5.5 MB | |
| Case | `metal_tool_chest` | not read | not read | |
| Cart | `industrial_storage_cart` | 1603 x 1103 x 1377 | 2.6 MB | book trolley |
| Clock | `vintage_grandfather_clock_01` | 997 x 795 x 2191 | 2.4 MB | |
| Clock | `mantel_clock_01` | 484 x 251 x 161 | 2.4 MB | |
| Statue | `marble_bust_01` | 272 x 300 x 515 | 0.9 MB | |
| Statue | `gothic_statue` | 1477 x 1564 x 1740 | 4.0 MB | |
| Props | `vintage_microscope` | 107 x 182 x 400 | 2.3 MB | |
| Props | `standing_chalkboard_01` | 920 x 759 x 1509 | 2.6 MB | |
| Props | `clipboard`, `office_notepads`, `stationery_supplies` | see API | 1.3 to 2.0 MB | desk clutter |
| Props | `wooden_bucket_01` | not read | 2.3 MB | leak catcher in a flooded cellar |

Note that the Poly Haven dimension arrays are listed in x, y, z order of the source file, not necessarily length, depth, height; `tools/props/props.py` sorts this out when the unit is fitted.

**Globes: none.** No Poly Haven model name or tag matches "globe" (checked all 522 model records at https://api.polyhaven.com/assets?t=models).

Other sources, checked on their own pages:

| Source | Item | Licence page | Use |
|---|---|---|---|
| OpenGameArt | https://opengameart.org/content/mini-library (yd, 3D, tags books paper library) | `License(s): CC0` | small library kit; quality unseen, check before use |
| OpenGameArt | https://opengameart.org/content/old-bookshelf-with-books (Paul Wortmann) | `License(s): CC0`, text says all files are public domain | blend + obj; texture quality unseen |
| OpenGameArt | https://opengameart.org/content/modern-library-books (DREAM_SEARCH_REPEAT) | `License(s): CC0` | modern book covers; wrong period for the Archive |
| Kenney | https://kenney.nl/assets/furniture-kit (140 files) | CC0 on page | flat low-poly style; clashes with the PBR Poly Haven look, so only a fallback |

## Sounds

Each URL was opened and its page links the CC0 1.0 deed; durations are from the page. Nothing was downloaded. Process with `tools/audio/process.py` (`--start/--end` to trim, groups for variants).

| Cue | Freesound page | Author | Length | Notes |
|---|---|---|---|---|
| Rail rumble | https://freesound.org/people/jakobhandersen/sounds/482198/ "Inside tunnel trains passing overhead, Helsinki" | jakobhandersen | 0:09.7 | muffled overhead rumble; pitch down for a heavy Stack |
| Rail rumble | https://freesound.org/people/FunWithSound/sounds/591883/ "Train Passing By 15 Pitch Change Deep Rumble" | FunWithSound | 0:18.3 | deep rumble body |
| Rail rumble | https://freesound.org/people/IENBA/sounds/849796/ "Low Old Wagon Metal Rattles" | IENBA | 0:37.1 | wheels and rattles; closest to a shelf on rails |
| Wood creak | https://freesound.org/people/Rudmer_Rotteveel/sounds/506665/ "Wood Creak Single V10" | Rudmer_Rotteveel | 0:01.2 | single hit |
| Wood creak | https://freesound.org/people/sthran/sounds/572645/ "Wood Floor Creak" | sthran | 0:00.8 | short |
| Wood creak | https://freesound.org/people/Breviceps/sounds/456814/ "Creaking Wood" | Breviceps | 0:02.2 | |
| Dripping water | https://freesound.org/people/Sclolex/sounds/177958/ "Water Dripping in Cave" | Sclolex | 1:29.8 | long loop source |
| Dripping water | https://freesound.org/people/editor_adp/sounds/162116/ "Water Dripping (Large Echo)" | editor_adp | 1:04.1 | big-room echo |
| Dripping water | https://freesound.org/people/radioscoopfi/sounds/607071/ "Water droplets falling in a cave" | radioscoopfi | 0:11.1 | |
| Page turn / rustle | https://freesound.org/people/xkeril/sounds/856497/ "Turning page (heavy paper)" | xkeril | 0:03.2 | |
| Page turn / rustle | https://freesound.org/people/bbrocer/sounds/382651/ "Paper Page Turn" | bbrocer | 0:05.4 | |
| Page turn / rustle | https://freesound.org/people/IENBA/sounds/656546/ "Page Turn" | IENBA | 0:05.9 | |
| Page turn / rustle | https://freesound.org/people/golovlev.sound/sounds/632220/ "paper, thin, tracing, blueprint, rustle" | golovlev.sound | 0:06.1 | |
| Page turn / rustle | https://freesound.org/people/Squidems/sounds/709961/ "Paper rustle" | Squidems | 0:33.1 | long, trim many |
| Hall room tone | https://freesound.org/people/kyles/sounds/635727/ "room tone medium quiet library carpeted empty" | kyles | 2:01.4 | the closest to a library |
| Hall room tone | https://freesound.org/people/be_a_hero_not_a_patriot/sounds/509477/ "room tone, big hall, heavy ventilation hum" | be_a_hero_not_a_patriot | 3:55.6 | has a hum; trim or filter |
| Hall room tone | https://freesound.org/people/xkeril/sounds/637118/ "Church with few people" | xkeril | 2:20.0 | |
| Hall room tone | https://freesound.org/people/NickTayloe/sounds/828537/ "Large Church Nave Ambience - Hallgrimskirkja" | NickTayloe | 18:03 | very long; trim a 30 s loop |
| Cave/vault bed | https://freesound.org/people/Nox_Sound/sounds/553080/ "Ambiance_Atmosphere_Cave_Loop_Stereo" | Nox_Sound | 0:57.0 | already a loop |
| Hall reverb (impulse) | https://freesound.org/people/Nox_Sound/sounds/648945/ "IR_Cathedral_5m_Stereo" | Nox_Sound | 0:02.6 | an impulse response; convolve offline with ffmpeg `afir` (the way `tools/audio/compose.py` already adds a room) instead of at runtime |
| Stack stopping | https://freesound.org/people/IENBA/sounds/648959/ "Heavy Book" | IENBA | 0:08.1 | thud; trim |
| Stack stopping | https://freesound.org/people/Valerie-Vivegnis/sounds/836521/ "Book dropped on table" | Valerie-Vivegnis | 0:02.5 | |
| Stack stopping | https://freesound.org/people/qubodup/sounds/743262/ "Stone Sliding" | qubodup | 0:00.4 | |
| Stack start/stop | https://freesound.org/people/NahuelMartinez/sounds/844329/ "Stone Slab Door Grinding - Heavy Rock Scrape" | NahuelMartinez | 0:16.9 | heavy scrape under a moving Stack |
| Stack grab/release | https://freesound.org/people/MasterSuite/sounds/667384/ "book pullout" and https://freesound.org/people/MasterSuite/sounds/667385/ "book pushin" | MasterSuite | 0:00.4 each | small shelf-handling foley |

Other Freesound hits that look right but were not opened (open and read the licence before use): `kyles/sounds/455854` (wood drawer slide), `qubodup/sounds/752215` and `752216` (squeaking drawer), `Ryding/sounds/109301` (bookshelf, find books), `f-r-a-g-i-l-e/sounds/434698` (Train Stopping; the page showed CC0 and 0:59.4).

Kenney Impact Sounds (https://kenney.nl/assets/impact-sounds, 130 files, CC0 on the page) holds wood and generic thuds for the Stack's stop, and needs no per-file check. Kenney Interface Sounds (https://kenney.nl/assets/interface-sounds) is not needed.

OpenGameArt: https://opengameart.org/content/old-pagesflip-with-sound (yd, 2D, `License(s): CC0`) is a page-flip sprite, not needed for audio.

**Gaps:** nothing found for a rolling shelf on rails (use the wagon rattle or train rumble); no globe model; no CC0 paper tileable beyond ambientCG.

## What to do next

1. Treat the lists above as candidates, not decisions: a human or build ticket should look at the previews (render pages on polyhaven.com and ambientcg.com) before fetching.
2. Fetch with the existing scripts (`polyhaven.py`, `props.py`, `process.py`). Add a manifest entry per file, as the repo already does.
3. Keep to the 110 MB plan above; the Archive is an indoor Map, so most of the floor is wood, stone and plaster and 2K is only worth it for one floor.
