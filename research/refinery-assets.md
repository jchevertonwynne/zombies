# Refinery assets: CC0 textures, models, sprites and sounds

Research for [#509](https://github.com/jchevertonwynne/zombies/issues/509) (map [#506](https://github.com/jchevertonwynne/zombies/issues/506)). Which CC0 sources could dress an oil refinery at night, and what fits the `assets/` budget?

Throwaway branch `research/refinery-assets`; no asset files were fetched into the repo. Researched 2026-11-23.

## Method and trust

- Every licence below was read **on the source's own page** (Poly Haven, ambientCG, Kenney, Freesound), not from a search snippet. Poly Haven's licence page says its assets are "licensed as CC0" with no attribution required ([polyhaven.com/license](https://polyhaven.com/license)); each asset page checked also shows "CC0". ambientCG says "All ambientCG assets are provided under the Creative Commons CC0 1.0 Universal License" ([docs.ambientcg.com/license](https://docs.ambientcg.com/license/)). Freesound licences are per sound, so each sound page was opened.
- Asset lists, dimensions and download sizes came from the first-party APIs (`api.polyhaven.com`, `ambientcg.com/api/v2/full_json`). Only JSON metadata was fetched. No asset files were downloaded.
- **Search snippets were wrong about licences more than once.** "Rusty Valve Turn" (scriotxstudios) looked CC0 and is CC BY 4.0; "flare.wav" (smidoid) is CC BY-NC 4.0. Always confirm on the page.
- Not verified: OpenGameArt pages return HTTP 403 to automated fetches, so the OpenGameArt fire sheets mentioned at the end are **unconfirmed** and not recommended. The Kenney pages state "Creative Commons CC0" but list no sprite names, so names beyond those already in the repo are not confirmed here.

## Budget

- `assets/` is **about 292 MB** (`du -sh`; 298,700 KB by `du -sk`; 479 files) against the **512 MB** cap that `cargo test provenance` enforces (`tools/texpack/README.md`). Headroom is about **210 MB**.
- Biggest users now: `materials` 209 MB (66 sets), `models` 35 MB, `enemies` 20 MB, `skies` 16 MB, `audio` 3.6 MB, `props` 3.6 MB, `decals` 1.5 MB, `sprites` 1.5 MB.
- Unit costs from the repo's tools: a 1K material set packs to about **3.3 MB** (BC7 colour, BC5 normal, BC7 ORM; `tools/texpack/README.md`); a packed Poly Haven prop is about **3 MB** (`assets/models/barrel_03` is 3.1 MB; `tools/props/props.py` decimates to ~2k triangles, one model per id); a 512 px Kenney sprite is about **136 KB**; a trimmed OGG sound is a few tens of KB (all of `audio/` is 3.6 MB).
- The shopping list in the Recommendation (7 materials, 9 props, 2 or 3 sprites, ~8 sounds) costs roughly **23 + 30 + 0.4 + 0.5 = ~55 MB**, well inside the headroom. Pick fewer if other lanes need slack.
- Tooling: `tools/texpack/polyhaven.py <id>` fetches and packs Poly Haven materials; `tools/props/props.py <id>` does Poly Haven models (needs Blender, a `UNITS` size entry and a matching `PropVariant`); `tools/audio/process.py` trims a recording to OGG and writes the manifest entry. ambientCG is already used for decals (`Leaking003..005`) by hand through `texpack --color/--opacity`; **there is no ambientCG material script**, so an ambientCG material means feeding `texpack` the PNGs by hand (or writing a small fetcher). Poly Haven is the cheaper path.

## Already in the repo (do not re-fetch)

- Materials (Poly Haven): `metal_plate_02`, `rusty_corrugated_iron`, `rusty_metal_02`, `blue_metal_plate`, `green_metal_rust`, `hangar_concrete_floor`, `smooth_concrete_floor`, `worn_concrete_floor`, `cracked_concrete`, `damaged_concrete_floor_03`, `painted_concrete`, `concrete_wall_006..008`, `asphalt_floor`, `brushed_concrete_04`, `granular_concrete`, `ribbed_concrete_wall`, `gravel_ground_01`.
- Decals: ambientCG `Leaking003/004/005`; procedural `oil_stain_1/2`.
- Models: `Barrel_01` (red oil/explosive drum), `barrel_03` (oil/fuel drum), `worn_metal_rack`, `metal_office_desk`, crate and sandbag kitbashes.
- Skies: `qwantani_night_puresky` (night), `evening_road_01_puresky`, `overcast_soil_puresky`, `belfast_sunset_puresky`, `kloofendal_overcast_puresky`.
- Kenney Particle Pack sprites in `assets/sprites/` (CC0, 512 px greyscale, brightness as alpha): `dirt_01..03`, `muzzle_01..05`, `smoke_02..04`, and **`fire_01`, `fire_02`, `spark_01`, `scorch_01`** (the last four added by PR #497 for blasts, `src/weapons/blast.rs`). Source: [kenney.nl/assets/particle-pack](https://kenney.nl/assets/particle-pack), page states "Creative Commons CC0".
- Audio already covering much of the ticket's cue list (Freesound, CC0): `fire_burn_1` (158476), `fire_pool_1` (650574, crackle loop), `napalm_idle_1` / `napalm_heat_1` (crackle, "roaring chimney"), `napalm_windup_1` (fire whoosh), `napalm_explode_1`, `flamethrower_1`, `nova_vent_1` (**gas hiss**, 575261), `nova_gas_1`, `door_open_1` (door creak), `bridge_grind_1` and `sapper_sabotage_1` (grinding metal), `generator_held_1` (generator whirr), `pap_process_1` (machine), `zipline_ride_1` (pulley), `footstep_metal_1/2`, `explosion_1/2`.

So of the cues the ticket names, **burning, gas hiss and a basic metal creak already exist**. The real gaps are a **flare roar**, a **valve turn**, **distant machinery / ambience**, and a longer **metal groan**.

## Materials (all CC0)

### Poly Haven (preferred: `polyhaven.py` handles it)

Not yet in the repo. A packed 1K set is ~3.3 MB.

| Asset | Page | Use |
|---|---|---|
| Container Side | [container_side](https://polyhaven.com/a/container_side) | Green corrugated shipping-container steel, chipped paint, ~1.9 m wide (page checked). Sheet-steel walls of sheds and control rooms. |
| Corrugated Iron 03 | [corrugated_iron_03](https://polyhaven.com/a/corrugated_iron_03) | Industrial corrugated sheet (2 m). Siblings: `corrugated_iron_02` (2.7 m), `worn_corrugated_iron` (1.8 m), `box_profile_metal_sheet` (2 m, box-profile cladding), `factory_wall` (3 m, tagged industrial, fence, corrugated). |
| Rusty Metal 04 / 05 | [rusty_metal_04](https://polyhaven.com/a/rusty_metal_04), [rusty_metal_05](https://polyhaven.com/a/rusty_metal_05) | Rusted sheet metal (2 m, 1.25 m) for tank skins and plate. Siblings: `rusty_metal_03`, `rusty_metal_sheet`, `rust_coarse_01`, `rusty_painted_metal`, `rusty_metal_grid`. |
| Metal Grate Rusty | [metal_grate_rusty](https://polyhaven.com/a/metal_grate_rusty) | Rusty grating (0.5 m). Catwalk floors, if the Material Set handles opacity; otherwise a solid-look grate. |
| Metal Plate | [metal_plate](https://polyhaven.com/a/metal_plate) | Painted, rusty, patterned iron plate (0.5 m); Poly Haven's nearest to tread plate. |
| Rebar Reinforced Concrete | [rebar_reinforced_concrete](https://polyhaven.com/a/rebar_reinforced_concrete) | Weathered concrete with rebar (2 m) for tank bunds and broken walls. Siblings: `concrete_floor_damaged_01` (5 m), `garage_floor`, `painted_concrete_02` (4 m). |
| Burned Ground 01 | [burned_ground_01](https://polyhaven.com/a/burned_ground_01) | "Burnt, gravelly ground with charred soil, ash and dried leaf litter", 1 m (page checked). The only true scorch set found on either site. Ground around fire pools. |
| Tarred Gravel, Road Damaged, Bitumen | [tarred_gravel](https://polyhaven.com/a/tarred_gravel), [road_damaged](https://polyhaven.com/a/road_damaged), [bitumen](https://polyhaven.com/a/bitumen) | Oily black yard surfaces. The repo already has `asphalt_floor`. |

### ambientCG (only if Poly Haven falls short; hand-packed)

Licence is site-wide CC0 (page and licence page checked). Source zip sizes (1K JPG, from the API) are not what lands in `assets/`; a packed set is ~3.3 MB regardless.

| Asset | Page | Source zip | Use |
|---|---|---|---|
| Metal 055 B | [Metal055B](https://ambientcg.com/a/Metal055B) | 4.5 MB | Rusty iron; 053 / 054 / 056 are siblings. |
| Rust 002 / 007 | [Rust002](https://ambientcg.com/a/Rust002), [Rust007](https://ambientcg.com/a/Rust007) | 8.2 / 7.6 MB | Heavy rust for pipes and tanks. |
| Corrugated Steel 005 / 009 | [CorrugatedSteel005](https://ambientcg.com/a/CorrugatedSteel005), [CorrugatedSteel009](https://ambientcg.com/a/CorrugatedSteel009) | 5.9 / 3.9 MB | 009 is dark grey photogrammetry. |
| Painted Metal 008 | [PaintedMetal008](https://ambientcg.com/a/PaintedMetal008) | not checked | Orange painted metal with rust, for "hot orange" accents. Siblings: 009 (yellow), 016 (yellow stripes), 017 (danger sign). |
| Metal Walkway 014 | [MetalWalkway014](https://ambientcg.com/a/MetalWalkway014) | 8.6 MB | Rusty walkway (photogrammetry). |
| Diamond Plate 009 | [DiamondPlate009](https://ambientcg.com/a/DiamondPlate009) | 9.8 MB | Steel tread plate; Poly Haven has no clean equivalent. |
| Pipe 001..004 | [Pipe001](https://ambientcg.com/a/Pipe001) | not checked | Pipe materials (tagged industry, metal, pipe). |
| Concrete 027..029, 035 | [Concrete027](https://ambientcg.com/a/Concrete027) | 7.6 MB | Cast bunker concrete; 035 (4.3 MB) is dark old plaster. |
| Lava 001..005 | [Lava001](https://ambientcg.com/a/Lava001) | not checked | Tagged fire, lava, magma. A candidate for glowing-hot pipe or a burning pool surface if the Material Set supports emissive. Whether each has an emissive map was not checked. |

ambientCG has **no** scorch, soot, char or burnt material (searches for those, plus ash, coal, burn and soot, returned nothing relevant). Scorch comes from Poly Haven `burned_ground_01`, the Kenney `scorch_01` already in the repo, and darkening in the look preset.

## Props (all Poly Haven models, CC0, glTF; `tools/props/props.py` pipeline)

"1K download" is the glTF plus textures from the API, before the repo's KTX2 packing (which lands near 3 MB each). Dimensions are from the API (metres, X x Y x Z as reported).

| Prop | Page | 1K download | Dimensions | Use |
|---|---|---|---|---|
| **Modular Industrial Pipes 01** | [modular_industrial_pipes_01](https://polyhaven.com/a/modular_industrial_pipes_01) | 4.8 MB | 1.4 x 0.3 x 1.95 | ~12K tris (page). "Modular industrial pipe set" with elbows, tees, crosses, straight runs and a **red-handled valve with gauge**. The most on-theme item: pipe racks and the Fire valves. Over the ~2k-triangle decimation target, so decimate or split pieces. |
| Modular Pipes | [modular_pipes](https://polyhaven.com/a/modular_pipes) | 5.8 MB | 4.2 x 0.46 x 2.4 | Longer modular pipe runs. |
| **Modular Fire Escape** | [modular_fire_escape](https://polyhaven.com/a/modular_fire_escape) | 4.8 MB | 6.7 x 1.4 x 9.8 | ~11K tris (page). Grated platforms, a straight ladder, angled stairs, railings. The nearest thing to **catwalk and stair pieces**. |
| Propane Tank | [propane_tank](https://polyhaven.com/a/propane_tank) | 2.2 MB | 0.34 x 0.34 x 0.55 | ~5K tris (page). Small pressure bottle. Sibling `small_lpg_tank` (2.8 MB, 0.41 x 0.64). |
| Portable Welding Cart | [portable_welding_cart](https://polyhaven.com/a/portable_welding_cart) | 2.8 MB | 0.84 x 0.65 x 1.56 | Gas bottles on a cart; maintenance clutter. |
| Old Military Compressor | [old_military_compressor](https://polyhaven.com/a/old_military_compressor) | 4.4 MB | 0.6 x 1.68 x 1.18 | Tank-on-cart compressor; reads as plant equipment. |
| Metal Jerrycan | [metal_jerrycan](https://polyhaven.com/a/metal_jerrycan) | 3.0 MB | 0.35 x 0.17 x 0.46 | Fuel can clutter. Sibling `metal_jerrycan_green`. |
| Flood Lights | [flood_lights](https://polyhaven.com/a/flood_lights) | 6.9 MB | 4.6 x 2.1 x 1.8 | Yard lighting rig. Siblings: `security_light` (2.4 MB), `caged_hanging_light` (3.1 MB), `industrial_caged_sconce`. |
| Overhead Crane | [overhead_crane](https://polyhaven.com/a/overhead_crane) | 7.3 MB | 12.5 x 4.0 x 5.1 | Gantry for a hangar Zone or the skyline. |
| Modular Airduct Circular 01 | [modular_airduct_circular_01](https://polyhaven.com/a/modular_airduct_circular_01) | 2.0 MB | 4.2 x 1.5 x 1.3 | Ducting and vents. |
| Concrete Road Barrier | [concrete_road_barrier](https://polyhaven.com/a/concrete_road_barrier) | 3.8 MB | 1.5 x 0.64 x 0.84 | Jersey barrier cover; `concrete_road_barrier_02` is a sibling. |
| Modular Chainlink Fence | [modular_chainlink_fence](https://polyhaven.com/a/modular_chainlink_fence) | 7.3 MB | 8.1 x 2.1 x 2.5 | Perimeter fence (check alpha handling). |
| Utility Box 01 | [utility_box_01](https://polyhaven.com/a/utility_box_01) | 2.0 MB | 0.52 x 0.43 x 1.12 | Control cabinet; `power_box_01` is a sibling. |
| Barrel Stove | [barrel_stove](https://polyhaven.com/a/barrel_stove) | 2.1 MB | 0.6 x 0.6 x 0.86 | Drum with fire; tags include night and fire. |
| Ladder Sectioned 01 | [ladder_sectioned_01](https://polyhaven.com/a/ladder_sectioned_01) | 2.6 MB | 0.66 x 0.18 x 2.13 | Ladders. |

Pages read directly for licence and triangle count: `modular_industrial_pipes_01`, `modular_fire_escape`, `propane_tank` (all "CC0"). The rest rely on Poly Haven's site-wide CC0 statement.

**Gap: no large storage tank.** Poly Haven's `industrial` category was listed in full and has no refinery-scale spherical or cylindrical tank, cracking tower or flare stack. Large tanks, towers and the flare stack are best **built from primitives in-engine** (cylinders and spheres textured with the rust and corrugated sets above, as the repo already kitbashes `props/crates` and `props/sandbags`). The small bottles and `Barrel_01` / `barrel_03` (already in the repo) will look wrong scaled to tank size. No CC0 Kenney or other industrial pack was confirmed for this.

## Fire and smoke sprites beyond what the repo already has

- **Already present:** `fire_01`, `fire_02`, `smoke_02..04`, `spark_01`, `scorch_01` (plus dirt and muzzle). These are single greyscale frames (brightness as alpha), tinted in code.
- **More from the same pack, no new source:** the Kenney Particle Pack page lists 80 files, CC0, 512 px tiles ([kenney.nl/assets/particle-pack](https://kenney.nl/assets/particle-pack)). The repo's `sprites/*` entries use `texpack --color --opacity` and cost ~136 KB each. Other families in the pack (more smoke, fire, flame, spark, scorch frames) are **not confirmed from the page**, which lists no sprite names; the build ticket should open the pack's `License.txt` and file list once before promising names.
- **Kenney Smoke Particles:** a separate CC0 pack, [kenney.nl/assets/smoke-particles](https://kenney.nl/assets/smoke-particles) (page shows "Creative Commons CC0", "Files 70x", tags particle, smoke, explosion, vfx). The best-confirmed source for more smoke; nothing else about its sprites was readable from the page.
- **OpenGameArt (unverified):** a web search listed CC0 fire sheets ("Animated flame / Fire sprite Sheet" by highwizard, "Animated Fire" by BenHickling, "Fireball Spritesheet" by Umplix). **Their pages returned HTTP 403**, so licences are unconfirmed, and several are pixel-art or tiny frames that would clash with the textured blasts of #497. Not recommended.
- **Animation is a code question, not an asset one.** All candidates are single frames; flicker for burning pipes and pools would come from layering and scaling sprites with the existing tint and lifetime code, or from a flipbook sheet if the Fire ticket wants one.

## Sounds (Freesound; every licence read on the sound's own page)

Source recordings are never committed; `tools/audio/process.py` trims to OGG. Each lands in the tens of KB, so eight cost well under 1 MB.

### CC0, confirmed

| Need | Sound | Licence (as shown) | Notes |
|---|---|---|---|
| **Flare / gas roar** | [PotatokingXII, Gas Flame (352869)](https://freesound.org/people/PotatokingXII/sounds/352869/) | CC0 | 1:44 gas-stove flame at varying intensity, 48 kHz mono. Cut a steady section for a loop. |
| Flare / gas roar | [NickPeeters, gas vuur dakwerken gasbrander (243763)](https://freesound.org/people/NickPeeters/sounds/243763/) | CC0 | 29.5 s propane roofing burner, 48 kHz stereo. Closest to a steady roar; loop by crossfade. |
| Flare / ignite | [neilraouf, Burner Gas Flame Sound (435643)](https://freesound.org/people/neilraouf/sounds/435643/) | CC0 | 7.6 s, 48 kHz 24-bit. |
| Big flame layer | [SamsterBirdies, Flamethrower (490166)](https://freesound.org/people/SamsterBirdies/sounds/490166/) | CC0 | 13.4 s blowtorch plus 120 Hz sine, FLAC. The repo already has a different flamethrower sound (`flamethrower_1`, 459884). |
| Burning loop | [HenKonen, Fire burning loop (757203)](https://freesound.org/people/HenKonen/sounds/757203/) | CC0 | 39 s oven fire, "Loopable track". Mostly redundant with `fire_pool_1` and `napalm_idle_1`. |
| Gas hiss / leak | [Jofae, Air Hiss (367125)](https://freesound.org/people/Jofae/sounds/367125/) | CC0 | 2.2 s pressurised air leak. Also [wubitog, Steam/hiss (234782)](https://freesound.org/people/wubitog/sounds/234782/), CC0, 1.5 s; [sniperous, Steam (365071)](https://freesound.org/people/sniperous/sounds/365071/), CC0, 7.8 s steam iron. All short, so loop or retrigger. `nova_vent_1` may already serve. |
| Metal creak | [ingudios, Metal Creak 3 (119467)](https://freesound.org/people/ingudios/sounds/119467/) | CC0 | 2.1 s "metal creaking long", 48 kHz mono. |
| Metal creak | [ingudios, Metal Creak 4 (119468)](https://freesound.org/people/ingudios/sounds/119468/) | CC0 | 3.1 s, high pitched. |
| Metal creak | [sophiehall3535, Metal creak (245945)](https://freesound.org/people/sophiehall3535/sounds/245945/) | CC0 | 2.5 s, 48 kHz 24-bit stereo. |
| Pipe clank | [BenDrain, Metal_Pipe_04 (487837)](https://freesound.org/people/BenDrain/sounds/487837/) | CC0 | 3.4 s pipe field recording. |
| Valve (short) | [ninoing, Valve Opening (698386)](https://freesound.org/people/ninoing/sounds/698386/) | CC0 | 0.65 s, a gas-tank valve. Same uploader as `nova_burst_1` already in the repo. |
| Valve (squeaky) | [JazzyBay, Hose Sounds (435052)](https://freesound.org/people/JazzyBay/sounds/435052/) | CC0 | 2:09 mono mp3; opens "a water valve that squeeks a lot", then spray and trickle. Use only the opening squeak. |
| Squeaking wheel | [martian, squeak speed up slow down (241602)](https://freesound.org/people/martian/sounds/241602/) | CC0 | 14.4 s "cartoon squeak ... metal wheel". Cartoonish; a fallback for a turning handwheel. |
| Distant machinery | [fimrod, Large Warehouse/Factory Ambience (278987)](https://freesound.org/people/fimrod/sounds/278987/) | CC0 | 22.4 s, 48 kHz stereo; the author says it "creates a perfect loop". Best ambience bed. |
| Distant machinery | [IanStarGem, Industrial/Factory Fans Loop (271096)](https://freesound.org/people/IanStarGem/sounds/271096/) | CC0 | 6.7 s loop. Also [Industrial/Factory Fans (269594)](https://freesound.org/people/IanStarGem/sounds/269594/), CC0, 43.9 s. |
| Machinery | [AudioPapkin, Factory machine 003 (512679)](https://freesound.org/people/AudioPapkin/sounds/512679/) | CC0 | 19.2 s field-recorded machine. |

### Looked at and rejected (not CC0)

- [scriotxstudios, Rusty Valve Turn (Open)](https://freesound.org/people/scriotxstudios/sounds/349111/) and [(Close)](https://freesound.org/people/scriotxstudios/sounds/349112/): **CC BY 4.0** (and sci-fi hatch sounds).
- [smidoid, flare.wav (161081)](https://freesound.org/people/smidoid/sounds/161081/): **CC BY-NC 4.0**.
- [PatrickLieberkind, Steam Leaking Stereo (213085)](https://freesound.org/people/PatrickLieberkind/sounds/213085/): **CC BY 4.0**. [visions68, steam pipe 2 (278998)](https://freesound.org/people/visions68/sounds/278998/): **CC BY 3.0**.
- [GoodListener, Valve Turning, Water Rushing 2 (322453)](https://freesound.org/people/GoodListener/sounds/322453/): **CC BY 4.0**.
- Not opened, so unknown and excluded: Speedenza "Gas torch flame effect" (174999; search said CC BY-NC 3.0), newlocknew "Otherworld industrial ambience", Thobias_Groove "Flame explosion".

CC BY and CC BY-NC are not CC0, and the rule is CC0 only (#506 Notes). Do not use them unless the dev changes that rule.

### Gaps with no confirmed CC0 match

- A **heavy handwheel or gate-valve turn** (metal creak plus a thunk). Options: layer `ninoing` valve opening, an `ingudios` creak and a `martian` squeak; accept the JazzyBay squeak; or compose it (the repo already synthesises cues in `tools/audio/compose.py` and `tools/gen-sounds`).
- A **true flare-stack roar** (low, sustained). Not found as one CC0 recording. The gas-flame and propane-burner recordings, pitched down and low-passed, are the best base; the repo's `napalm_heat_1` (crackle with roaring chimney) already points the same way.

## Recommendation (for the dev to take or leave)

1. **Materials:** add from Poly Haven `container_side`, `corrugated_iron_03` (or `factory_wall`), `rusty_metal_04`, `metal_grate_rusty`, `rebar_reinforced_concrete`, `burned_ground_01`, `tarred_gravel`: 7 sets, ~23 MB, one `polyhaven.py` call. Add ambientCG `DiamondPlate009` only if the look wants tread plate (hand-packed).
2. **Props:** `modular_industrial_pipes_01` (has a valve), `modular_fire_escape` (catwalks and stairs), `modular_pipes`, `propane_tank`, `portable_welding_cart`, `flood_lights`, `concrete_road_barrier`, `utility_box_01`, `metal_jerrycan`: ~30 MB. **Build tanks, towers and the flare stack from primitives**; no CC0 refinery-scale model was found.
3. **Sprites:** reuse `fire_01/02`, `smoke_02..04`, `spark_01`, `scorch_01`. For more smoke, open the Kenney Smoke Particles pack (CC0) and add 2 or 3 sprites at ~136 KB each. No OpenGameArt sheet is needed.
4. **Sounds:** from the CC0 table, add the fimrod factory loop (ambience), a PotatokingXII or NickPeeters burner cut (flare roar), `ingudios` creaks (structure groan), and `ninoing` + JazzyBay valve sounds (valve turn), plus `Air Hiss` as a leak cue if `nova_vent_1` is not enough.
5. **Budget:** about 55 MB for all of the above, against ~210 MB headroom.
6. **Co-op:** these are static client-side assets. Every Player loads the same files by id, so a new Prop or Material id is only a `Layout` data matter shared by Host and clients; nothing here is solo-only.
