# Sanatorium assets (CC0)

Research for [#504](https://github.com/jchevertonwynne/zombies/issues/504), under the [Sanatorium map #501](https://github.com/jchevertonwynne/zombies/issues/501). Which CC0 sources could dress a derelict hospital: material sets, Props and sounds?

Nothing was downloaded. Every item below was checked on its source page (2026-11-23); links, licences and sizes only.

## Verdict

Everything needed except a few hospital-specific Props exists as CC0. Poly Haven covers the tile, plaster, rust, linoleum and painted-steel Material Sets and most Props. ambientCG adds more tile and painted-plaster variety and a biohazard sign. Freesound covers every sound asked for. Gaps: no CC0 **gurney**, **hospital trolley** or **locker** on Poly Haven, ambientCG, Kenney or OpenGameArt (see Gaps). A full dressing costs about 100 MB of the 220 MB left, so the budget is not the constraint.

## How each licence was confirmed

- **Poly Haven**: each asset page below shows the label "CC0" (read page by page, 24 pages plus `wheelchair_01` and `old_linoleum_flooring_01`). The [licence page](https://polyhaven.com/license) says all HDRIs, textures and models are CC0. Catalogue, tags, sizes, triangle counts and authors come from the first-party API (`https://api.polyhaven.com/assets?t=textures|models`, `/files/<id>`, `/info/<id>`).
- **ambientCG**: the [licence page](https://docs.ambientcg.com/license/) says "All ambientCG assets are provided under the Creative Commons CC0 1.0 Universal License". Individual asset pages show no licence label, only an FAQ link (checked on [Tiles133D](https://ambientcg.com/view?id=Tiles133D)), so the licence page is the evidence. Catalogue and sizes come from `https://ambientcg.com/api/v2/full_json`.
- **Kenney**: [support page](https://kenney.nl/support) says all game assets on its asset pages are CC0. Nothing hospital-specific was found, so none is listed.
- **Freesound**: every sound below had its own page read and shows "Creative Commons 0". Two have extra text on the page, flagged in the table.
- **OpenGameArt**: only searched; see Gaps.

## Budget

- `assets/` is **292 MB** on disk now (`du -sh assets`); the limit is 512 MB (#429), so **about 220 MB** is free.
- Existing cost per item (what `tools/texpack` and `tools/props` commit, KTX2): a 1K Material Set is about **3.6 MB** (`assets/materials/*`: 3.6 to 3.8 MB), a Prop model is about **3.1 to 3.5 MB** (`assets/models/*`, `model.gltf` decimated to about 2k triangles plus colour, normal and ORM KTX2). A sound is tens of KB as OGG q4 (`tools/audio/process.py`).
- The sizes in the tables below are the **source downloads**, which are never committed. They are only a guide to fetch time.
- Pipeline: `tools/texpack/polyhaven.py <id>` (Poly Haven materials, `--res 1k`, `--res 2k` for a hero floor) and `tools/props/props.py` (Poly Haven models, with `UNITS` sizes and a `PropVariant` in `src/maps/map/mod.rs`). ambientCG has no fetch script yet; the manifest already holds 3 ambientCG entries, so the source field has precedent. Audio goes through `tools/audio/process.py`.
- Suggested full set: 16 Material Sets (about 58 MB) + 14 Props (about 46 MB) + about 25 sounds (under 3 MB) is **about 107 MB**, leaving about 113 MB for the later Maps.

## Material Sets

Sources are Poly Haven unless noted. "1K PNG" is the source download. Poly Haven authors are on each page.

### Tile

| Asset | Link | Notes | 1K PNG |
|---|---|---|---|
| dirty_tiles | [page](https://polyhaven.com/a/dirty_tiles) | Indoor floor tiles, dirty, grout lines. Floor candidate. | 14.3 MB |
| long_white_tiles | [page](https://polyhaven.com/a/long_white_tiles) | Wall tile, weathered, chipped, discoloured. Ward wall. | 14.3 MB |
| interior_tiles | [page](https://polyhaven.com/a/interior_tiles) | Indoor floor tile, grout lines. | 14.1 MB |
| worn_tile_floor | [page](https://polyhaven.com/a/worn_tile_floor) | Old, weathered floor tile. | 15.4 MB |
| ambientCG Tiles133D | [page](https://ambientcg.com/view?id=Tiles133D) | Tags: broken, derelict, dirty, old, white. Best "tile gone bad" match. | 13.7 MB |
| ambientCG Tiles133C | [page](https://ambientcg.com/view?id=Tiles133C) | Broken, damaged, dirty; sibling of 133D. | 13.0 MB |
| ambientCG Tiles136B / 136C | [136B](https://ambientcg.com/view?id=Tiles136B), [136C](https://ambientcg.com/view?id=Tiles136C) | Kitchen tile, dirty / broken. Good for a ward kitchen. | 12.5 / 13.8 MB |
| ambientCG Tiles141 | [page](https://ambientcg.com/view?id=Tiles141) | Bathroom, beige, dirty. | 15.3 MB |
| ambientCG Tiles142 | [page](https://ambientcg.com/view?id=Tiles142) | Aged bathroom tile, beige. | 15.0 MB |
| ambientCG Tiles105 / 107 | [105](https://ambientcg.com/view?id=Tiles105), [107](https://ambientcg.com/view?id=Tiles107) | Plain white tile; small. Clean base to stain with `texpack --stain`. | 8.3 MB |
| ambientCG Tiles132D | [page](https://ambientcg.com/view?id=Tiles132D) | Blue bathroom tile, broken. Gives the "white tile gone green" a cool accent. | 14.8 MB |
| ambientCG Tiles053 | [page](https://ambientcg.com/view?id=Tiles053) | Tags: green, rough, small. Candidate for the green tile. | 16.8 MB |

The map says "white tile gone green". No asset is tagged green-stained white tile, so use `long_white_tiles` or `Tiles133D` and let `tools/texpack --stain` (already used for 2 entries) tint them, or take `Tiles053`.

### Plaster and paint

| Asset | Link | Notes | 1K PNG |
|---|---|---|---|
| white_rough_plaster | [page](https://polyhaven.com/a/white_rough_plaster) | Broken, chipped, damaged, indoor. Small download. | 5.5 MB |
| worn_cracked_plaster | [page](https://polyhaven.com/a/worn_cracked_plaster) | Cracked, damaged. | 15.0 MB |
| decrepit_wallpaper | [page](https://polyhaven.com/a/decrepit_wallpaper) | Peeling, discoloured. Ward walls. | 14.9 MB |
| scuffed_cement | [page](https://polyhaven.com/a/scuffed_cement) | Indoor concrete floor, scuffed. | 14.6 MB |
| ambientCG PaintedPlaster017 | [page](https://ambientcg.com/view?id=PaintedPlaster017) | Painted plaster, small. | 9.2 MB |
| ambientCG PaintedPlaster018 | [page](https://ambientcg.com/view?id=PaintedPlaster018) | Damaged painted plaster. | 17.6 MB |
| ambientCG PaintedPlaster003 | [page](https://ambientcg.com/view?id=PaintedPlaster003) | Green painted plaster: the classic hospital green. | 15.8 MB |
| ambientCG Plaster007 | [page](https://ambientcg.com/view?id=Plaster007) | Broken, old paint. | 17.1 MB |
| ambientCG Concrete032 / 033 | [032](https://ambientcg.com/view?id=Concrete032), [033](https://ambientcg.com/view?id=Concrete033) | Bunker concrete, green mould. Basement and boiler house. | 17.6 / 17.5 MB |
| ambientCG Wallpaper002B / C | [B](https://ambientcg.com/view?id=Wallpaper002B), [C](https://ambientcg.com/view?id=Wallpaper002C) | Mouldy woodchip wallpaper. | 15.6 / 15.8 MB |

Already in `assets/materials`: `peeling_painted_wall`, `damaged_plaster`, `white_plaster_rough_01`, `painted_concrete`, `concrete_moss`, `smooth_concrete_floor`, `old_mosaic_floor`, `floor_tiles_06`, `metal_plate`/`_02`, `green_metal_rust`, `rusty_metal_02`, `blue_metal_plate`, `rusty_corrugated_iron`. Some of the Sanatorium look is free.

### Linoleum

| Asset | Link | Notes | 1K PNG |
|---|---|---|---|
| old_linoleum_flooring_01 | [page](https://polyhaven.com/a/old_linoleum_flooring_01) | Old linoleum, glossy, vinyl. CC0, 2 m wide (Charlotte Baglioni). The only true match. | about 14 MB |
| linoleum_brown | [page](https://polyhaven.com/a/linoleum_brown) | Brown linoleum, wood look. Second choice. | 12.6 MB |

ambientCG has no linoleum or vinyl asset (searches for "linoleum", "lino" and "vinyl" returned nothing).

### Rust and painted steel

| Asset | Link | Notes | 1K PNG |
|---|---|---|---|
| rusty_painted_metal | [page](https://polyhaven.com/a/rusty_painted_metal) | Weathered painted sheet. | 15.2 MB |
| painted_metal_shutter | [page](https://polyhaven.com/a/painted_metal_shutter) | Painted, stained, scratched. Doors and shutters. | 14.2 MB |
| rusty_metal_grid | [page](https://polyhaven.com/a/rusty_metal_grid) | Painted, rusted sheet. | 14.2 MB |
| ambientCG PaintedMetal014 | [page](https://ambientcg.com/view?id=PaintedMetal014) | Grey painted metal. Bed frames, fittings. | 18.3 MB |
| ambientCG PaintedMetal006 | [page](https://ambientcg.com/view?id=PaintedMetal006) | Green painted metal. | 18.5 MB |
| ambientCG PaintedMetal018 | [page](https://ambientcg.com/view?id=PaintedMetal018) | Tags: aid, cross, first. First-aid sign panel. | 18.4 MB |
| ambientCG Rust001 | [page](https://ambientcg.com/view?id=Rust001) | A decal-style rust. Small. | 4.9 MB |
| ambientCG Metal041B | [page](https://ambientcg.com/view?id=Metal041B) | Rusted iron. | 16.3 MB |

### Signage

| Asset | Link | Notes | 1K PNG |
|---|---|---|---|
| ambientCG Sign004 | [page](https://ambientcg.com/view?id=Sign004) | Biohazard sign. Quarantine Door marking. | 7.7 MB |
| ambientCG Sign023 | [page](https://ambientcg.com/view?id=Sign023) | Apocalypse / biology / danger sign. | 8.4 MB |

## Props (Poly Haven models)

All verified CC0 on their page. Triangle counts are the source; `tools/props/decimate.py` cuts to about 2k, so the committed cost is about 3.3 MB each regardless. Dimensions in metres.

| Asset | Link | Role | Size / tris |
|---|---|---|---|
| wheelchair_01 | [page](https://polyhaven.com/a/wheelchair_01) | Wheelchair (tags: infirmary, hospital). Garreth Dean. | 0.82 x 1.09 x 1.1, 40k |
| old_bed_frame | [page](https://polyhaven.com/a/old_bed_frame) | Rusty metal bed frame. Best ward bed. | 0.9 x 2.0 x 1.2, 50k |
| vintage_day_bed | [page](https://polyhaven.com/a/vintage_day_bed) | Day bed. Stand-in for a gurney. | 1.97 x 0.85 x 1.13, 2.7k |
| medical_box | [page](https://polyhaven.com/a/medical_box) | Worn green metal first-aid box. | 0.53 x 0.35 x 0.1, 3.7k |
| medical_tape | [page](https://polyhaven.com/a/medical_tape) | Tape roll. Clutter. | 0.05, 1.4k |
| vintage_crutches_01 | [page](https://polyhaven.com/a/vintage_crutches_01) | Crutches. | 0.4 x 0.22 x 1.61, 10.6k |
| tool_cart | [page](https://polyhaven.com/a/tool_cart) | Rusted metal cart. Stand-in for a trolley. | 1.27 x 0.75 x 0.96, 29k |
| industrial_storage_cart | [page](https://polyhaven.com/a/industrial_storage_cart) | Metal storage cart. Second trolley. | 1.6 x 1.1 x 1.38, 19k |
| mounted_fluorescent_lights | [page](https://polyhaven.com/a/mounted_fluorescent_lights) | Fluorescent tube fitting: the corridor lamp. | 0.91 x 0.65 x 0.04, 18k |
| caged_hanging_light | [page](https://polyhaven.com/a/caged_hanging_light) | Caged ceiling lamp. | 1.16 x 0.31 x 0.75, 23k |
| fire_alarm | [page](https://polyhaven.com/a/fire_alarm) | Wall alarm. Quarantine alarm panel. | 0.1 x 0.03 x 0.14, 2.7k |
| steel_frame_shelves_01 | [page](https://polyhaven.com/a/steel_frame_shelves_01) | Steel shelving. Stand-in for a locker bank. | 1.1 x 0.5 x 2.14, 4.3k |
| drawer_cabinet | [page](https://polyhaven.com/a/drawer_cabinet) | Metal-look drawer cabinet. | 1.14 x 0.49 x 1.88, 26k |
| WetFloorSign_01 | [page](https://polyhaven.com/a/WetFloorSign_01) | Wet-floor sign, only 228 triangles. | 0.3 x 0.36 x 0.63 |

Already in `assets/models`: `metal_office_desk`, `worn_metal_rack`, `painted_wooden_cabinet_02`, `vintage_wooden_drawer_01`, `Barrel_01`.

Also on Poly Haven and CC0 if more are needed (listed, page not read): `industrial_microscope`, `security_camera_01`, `portable_generator`, `metal_trash_can`, `ceiling_fan`, `SchoolChair_01`/`SchoolDesk_01`, `korean_fire_extinguisher_01`, `power_box_01`, `metal_stool_01`/`_03`, `vintage_oil_lamp`. Re-read the licence on the page when fetching.

Hospital-lamp note: no operating-theatre lamp exists; `mounted_fluorescent_lights` and `caged_hanging_light` carry the "flickering fluorescents" look.

## Gaps

- **Gurney / stretcher, hospital trolley, locker**: none found as CC0 on Poly Haven (full tag and name search of 522 models), ambientCG (textures only) or Kenney. Substitutes: `vintage_day_bed`, `tool_cart` / `industrial_storage_cart`, `steel_frame_shelves_01`. A gurney and a locker could be kitbashed from existing Props (`tools/props/kitbash.py` exists) or modelled in this repo and released CC0, like the Zombie bodies.
- **OpenGameArt**: the one hospital bed there, [Small Hospital Bed](https://opengameart.org/content/small-hospital-bed), is **CC-BY 3.0, not CC0**: excluded. CC0 beds are generic: [Bed (low poly)](https://opengameart.org/content/bed-low-poly) (Clint Bellanger, 122 tris) and [Low poly bed](https://opengameart.org/content/low-poly-bed-0). Both seen only in search results, licence not read on the page; not recommended over `old_bed_frame`.
- **"Green" hospital tile**: see the Tile note.

## Sounds (Freesound)

Each page was read and shows **Creative Commons 0**. The manifest convention (`assets/manifest.json`) uses the `freesound.org/people/<user>/sounds/<id>/` URL, the HQ preview trimmed and normalised by `tools/audio/process.py`. Freesound is the only audio source the manifest already uses (135 entries). Sizes are the source file.

### Fluorescent buzz and flicker

| Sound | Link | Length | Source file |
|---|---|---|---|
| Flickering Fluorescent Light with Background Hum, seamless loop (kentspublicdomain) | [777053](https://freesound.org/people/kentspublicdomain/sounds/777053/) | 9.0 s | WAV 775 KB |
| Fluorescent lightbulb hum buzz, extended (Developer_Jake) | [638895](https://freesound.org/people/Developer_Jake/sounds/638895/) | 37.9 s | MP3 1.4 MB |
| Fluorescent light buzzing (Rvgerxini) | [474312](https://freesound.org/people/Rvgerxini/sounds/474312/) | 3.9 s | MP3 74 KB |
| Fluorescent Lightbulb Hum (ftpalad) | [119910](https://freesound.org/people/ftpalad/sounds/119910/) | 31.3 s | AIFF 8.6 MB |
| Lights Flicker On (mmaruska) | [232447](https://freesound.org/people/mmaruska/sounds/232447/) | 15.0 s | WAV 8.5 MB |
| Lights Flicker on and some electrical Noises (mmaruska) | [232446](https://freesound.org/people/mmaruska/sounds/232446/) | 29.8 s | WAV 16.9 MB |

Best pair: the 777053 loop as the lamp loop, 232447 / 232446 sliced for flicker one-shots.

### Distant machinery and ward ambience

| Sound | Link | Length | Source file | Note |
|---|---|---|---|---|
| room tone bassy hospital stairwell hum +distant noises (kyles) | [451914](https://freesound.org/people/kyles/sounds/451914/) | 2:00 | FLAC 17 MB | Best ward bed-layer. |
| Steel mill low frequency drone (sarana) | [567148](https://freesound.org/people/sarana/sounds/567148/) | 37.5 s | WAV 6.3 MB | Distant machinery; not a loop, so loop it. |
| Low Frequency Vibrating Drone ... (bassimat) | [860781](https://freesound.org/people/bassimat/sounds/860781/) | 7:03 | WAV 116 MB | Synthetic; only trim a section. |
| HOSPITAL ICU HALLWAY ATMO (tferrino) | [679047](https://freesound.org/people/tferrino/sounds/679047/) | 2:32 | M4A 5.4 MB | **Caution:** badge is CC0 but the description says not to post the raw files elsewhere or redistribute them as stock. Repo commits only a processed clip, but this is the one to avoid if unsure. |
| emergency room hospital (bruno.auzet) | [527438](https://freesound.org/people/bruno.auzet/sounds/527438/) | 3:29 | WAV 38 MB | Has voices and doors; not suitable for a derelict ward. |
| Room Tone, Sci Fi, Large Hall (Kinoton) | [353159](https://freesound.org/people/Kinoton/sounds/353159/) | 3:00 | WAV 49 MB | **Caution:** description mentions a licence agreement even though the badge says CC0. Prefer not to use. |
| Heart Monitor Beep (samfk360) | [148897](https://freesound.org/people/samfk360/sounds/148897/) | 27.9 s | MP3 1.1 MB | For a monitor prop's cue. |

Not CC0, excluded (seen in search results): nixeno oxygen-system hospital ambience and Moulaythami "Roomtone Hospital, A" (Attribution), Benboncan and FreqMan heart monitors (Attribution 4.0).

### Door seals and airlocks (Quarantine Doors)

| Sound | Link | Length | Source file |
|---|---|---|---|
| Hatch Seal (Paul368) | [264063](https://freesound.org/people/Paul368/sounds/264063/) | 5.5 s | WAV 774 KB. Seal closing with air. Best fit. |
| Sci-Fi Doors-Airlock Sound Effect (RyanKingArt) | [607265](https://freesound.org/people/RyanKingArt/sounds/607265/) | 11.3 s | MP3 266 KB. Open and close. |
| Sci-Fi Mechanical Door (CheatinSloth) | [739197](https://freesound.org/people/CheatinSloth/sounds/739197/) | 6.6 s | WAV 2.4 MB. Pneumatic piston, metallic clang. |
| Lab Door Open and Close Industrial Sci-Fi (kentspublicdomain) | [799198](https://freesound.org/people/kentspublicdomain/sounds/799198/) | 6.0 s | WAV 787 KB. Open and close in one clip. |

These are sci-fi, so they will sound more futuristic than a 1950s sanatorium; layer with the repo's existing `door_open` and `bridge_clunk` families, or trim to the hiss and the bolt.

### Quarantine alarm

| Sound | Link | Length | Source file |
|---|---|---|---|
| Alarm (melokacool) | [613653](https://freesound.org/people/melokacool/sounds/613653/) | 16.0 s | WAV 5.9 MB. Civil-defence style. |
| Siren warning: NBC Alarm (Breviceps) | [533588](https://freesound.org/people/Breviceps/sounds/533588/) | 1:19 | WAV 3.4 MB. Real 1990s CBRN warning siren; fits "Quarantine". |
| Emergency Alarm (Mozfoo) | [699248](https://freesound.org/people/Mozfoo/sounds/699248/) | 1:05 | WAV 5.5 MB. Klaxon for a secret facility. |
| Emergency Siren (onderwish) | [470504](https://freesound.org/people/onderwish/sounds/470504/) | 31.0 s | WAV 10.4 MB |
| Alarm 0 (SieuAmThanh) | [397787](https://freesound.org/people/SieuAmThanh/sounds/397787/) | 22.9 s | WAV 7.7 MB. Reversed, stretched klaxon. |

Pick: Mozfoo for the "doors are sealing" klaxon, Breviceps for the "Quarantine begins" siren.

### Trolley and wheels

| Sound | Link | Length | Source file |
|---|---|---|---|
| Squeaky Wheels (Shamewap) | [389687](https://freesound.org/people/Shamewap/sounds/389687/) | 4.8 s | WAV 677 KB |
| squeaky.wav (servicedesign) | [219268](https://freesound.org/people/servicedesign/sounds/219268/) | 32.6 s | WAV 2.7 MB |
| heavy metal trolley rolling rumbling (martian) | [216605](https://freesound.org/people/martian/sounds/216605/) | 29.6 s | WAV 5.4 MB |
| rolling cart 002 (yottasounds) | [232136](https://freesound.org/people/yottasounds/sounds/232136/) | 6.4 s | WAV 3.5 MB |

## Open points for the dev

- The `tferrino` and `Kinoton` pages carry extra terms next to the CC0 badge; the rest are plain CC0. Avoid those two unless the extra text is judged harmless.
- ambientCG assets have no per-page licence label; the evidence is the site licence page. The manifest's `licence` field would read "CC0" with the asset URL as `source`.
- Hospital Props without a CC0 source (gurney, trolley, locker) need substitutes or hand-made models; see Gaps.
