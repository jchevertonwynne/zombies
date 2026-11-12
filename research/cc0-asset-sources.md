# Which CC0 libraries can supply each kind of asset?

Research for [#274](https://github.com/jchevertonwynne/zombies/issues/274) under map [#272](https://github.com/jchevertonwynne/zombies/issues/272). Checked 2026-11-12 against each library's own licence page, pack page or first-party API. Counts come from those APIs and searches on that date.

## Answer in one screen

| Asset kind | Best CC0 source(s) | Fit for gritty WaW/BO1 | Verdict |
|---|---|---|---|
| Environment PBR textures | Poly Haven (869 textures), ambientCG (2,041 materials) | Excellent: photo-scanned rust, corrugated iron, plaster, brick, concrete, planks | **Covered** |
| Trim and decals | ambientCG Decals (127) and Atlases (61) | Good for leaks, grime, road lines, manholes, graffiti, tape, cardboard. No blood, no bullet holes, no posters | **Partly covered.** Blood, bullet holes and signage must be authored |
| Props | Poly Haven models (521, 97 tagged *industrial*) | Excellent: realistic, worn, glTF with 1k–8k textures | **Covered** |
| Interactables (Box, Pack-a-Punch, Perks, Doors, Barriers) | None ready-made. Kitbash from Poly Haven props and ambientCG materials | Achievable | **Gap: kitbash** |
| Weapons | Poly Haven (bolt-action rifle, service pistol, ammo box, old military crate). Sketchfab CC0 museum scans (Mosin 1891, Vis P35(p)) | Realistic but few, and the scans are not game-ready | **Mostly a gap** |
| First-person arms | OpenGameArt CC0 (crude rigged arms, low-poly) | Poor | **Gap** |
| Rigged, animated zombies | Quaternius Animated Zombie Pack, Zombie Apocalypse Kit, Universal Animation Library 2 (zombie locomotion). A few OpenGameArt CC0 zombies | Stylised low-poly. Animations retarget; the meshes don't fit the look | **Animation covered, meshes a gap** |
| Dogs (Hellhounds) and specials | Quaternius Zombie Apocalypse Kit (2 animated dogs), Ultimate Animated Animal Pack | Stylised | **Gap for a gritty look** |
| Sky HDRIs | Poly Haven (997 HDRIs, 61 tagged *night*), ambientCG (437 HDRIs) | Excellent | **Covered** |
| Audio | Freesound with the CC0 filter, Kenney audio packs, OpenGameArt CC0 | Good for SFX. Nothing for jingles or announcer lines | **SFX covered. Music and announcer are a gap** |

Biggest caveats:

1. **Quaternius's licence is contradictory right now.** Its pack pages and FAQ say CC0, but quaternius.com/license.html now publishes a non-CC0 "Quaternius Asset License (QAL) v1.0", last updated 8/28/2026. See [Licence findings](#licence-findings).
2. **ShareTextures is not CC0** despite the name it uses. It runs a "Custom CC0" with a ban on redistribution and on "violent content". It is out.
3. **Freesound, OpenGameArt and Sketchfab set the licence per asset.** Only the CC0 filter is safe, and each asset's own page must be recorded when it is picked.

## Licence findings

### Clean CC0 (whole library)

- **Poly Haven.** "Our assets are all licensed as CC0." This covers every HDRI, texture and model, and no attribution is required. Only the site's own content (logos, renders, text) is excluded. Source: <https://polyhaven.com/license>.
- **ambientCG.** "All ambientCG assets are provided under the Creative Commons CC0 1.0 Universal License." This applies to the downloadable files and the preview renders. Source: <https://docs.ambientcg.com/license/>.
- **Kenney.** "All game assets on the asset pages are public domain licensed (CC0)." The one exception is the Kenney logo. Source: <https://kenney.nl/support>. Each pack page also states "License: Creative Commons CC0", for example <https://kenney.nl/assets/impact-sounds>, <https://kenney.nl/assets/voiceover-pack>, <https://kenney.nl/assets/blaster-kit> and <https://kenney.nl/assets/survival-kit>.

### Contradictory: Quaternius

- The FAQ says "All models are under the CC0 License… attribution is not necessary." Source: <https://quaternius.com/faq.html>.
- Every pack page checked says "License CC0": Animated Zombie, Zombie Apocalypse Kit, Ultimate Guns, Animated Guns, Ultimate Animated Animals, Universal Animation Library 2, Downtown City MegaKit and Universal Base Characters. Example: <https://quaternius.com/packs/zombieapocalypsekit.html>.
- The site's **License** page now publishes the **Quaternius Asset License (QAL) v1.0**, "Last updated: 8/28/2026". It is **not CC0**:
  - "We retain all intellectual property rights in the Assets."
  - "You may not … redistribute the Assets … as a standalone asset, asset pack … whether for free or for payment."
  - The licence can terminate on breach.
  - §7: "Changes will not apply retroactively to Assets you've already obtained under an earlier version."
  
  Source: <https://quaternius.com/license.html>.
- **Implication.** The free downloads still label themselves CC0, and CC0 dedications can't be revoked. Even so, the brief is "CC0 only", and the site-wide terms now say otherwise. Treat Quaternius as **usable only from a copy provably released as CC0**:
  - keep the pack page and any licence file bundled in the zip next to the asset in git;
  - or take the OpenGameArt mirror, which Quaternius uploaded as CC0. For example, Universal Animation Library 2 at <https://opengameart.org/content/universal-animation-library-2> is labelled CC0.
  
  The repo is private today, which lowers the redistribution risk. QAL §3a would still matter if the repo, raw assets included, ever went public. The dev should decide whether to accept Quaternius.

### Not CC0: excluded

- **ShareTextures.** "Custom CC0 (Creative Commons Zero) license with specific restrictions". The restrictions are "No asset redistribution … without our written permission" and "No use in hateful, violent, or illegal content", and "CC0 only applies to assets downloaded directly from our site". A zombies shooter plus raw files committed to git conflicts with both. Source: <https://www.sharetextures.com/p/license>.

### Per-asset licences: filter, then record each asset's own licence

- **Freesound.** The uploader picks a licence for each sound: CC0, CC-BY, CC-BY-NC, or legacy Sampling+. Only CC0 is in scope. Use the search facet `license:"Creative Commons 0"`. Source: <https://freesound.org/help/faq/>.
- **OpenGameArt.** Licences are set per submission, and some submissions list several. OGA says it "make[s] an effort to verify that PD content is actually in the public domain", but that is not a guarantee. Source: <https://opengameart.org/content/faq>. Example risk: "Hazmat Zombie" is CC0 on OGA, but its description says it is "based on 'Lowpoly Zombie' by tigertowel" from BlendSwap, so its provenance depends on that upstream licence (<https://opengameart.org/content/hazmat-zombie>). Prefer submissions whose author is the original creator.
- **Sketchfab.** Every model carries its own licence: CC0, CC-BY, BY-SA, BY-ND, the NC variants, Standard or Editorial. Filter on `license=cc0` and recheck on the model page. Source: <https://api.sketchfab.com/v3/licenses>.

## Formats and resolutions

| Source | Textures | Models | HDRIs |
|---|---|---|---|
| Poly Haven | Maps: Diffuse, nor_gl, nor_dx, Rough, AO, Displacement, ARM (packed AO/Rough/Metal), spec. JPG/PNG/EXR at 1k/2k/4k/8k, plus a glTF and a .blend per material | glTF, FBX, USD and .blend, with their textures at 1k–8k | HDR and EXR at 1k/2k/4k/8k/16k |
| ambientCG | Zips in 1K/2K/4K/8K × JPG/PNG. Asset types: Material, Decal, Atlas, Photo Texture, Substance, HDRI, 3D Model, Terrain | Few that matter here | Equirectangular |
| Kenney | n/a | glTF/FBX/OBJ, flat-coloured or atlas-coloured | n/a |
| Quaternius | Atlas-textured | FBX, OBJ, .blend; glTF/GLB on newer packs (Zombie Apocalypse Kit, Animated Animals, Downtown City MegaKit, UAL2) | n/a |
| Freesound | WAV dominates CC0 results (e.g. 764 of 928 for "zombie"), then MP3, FLAC and OGG | | |

Sources:
- Poly Haven API, e.g. <https://api.polyhaven.com/files/rusty_metal_02>, <https://api.polyhaven.com/files/bolt_action_rifle_7_62>, <https://api.polyhaven.com/files/moonless_golf>
- ambientCG API, <https://ambientcg.com/api/v2/full_json?id=Metal032&include=downloadData>
- ambientCG asset types, <https://docs.ambientcg.com/asset-types/>
- the Quaternius pack pages linked above

**Budget sense** for the 300 MB cap on `assets/`. Poly Haven's `rusty_metal_02` diffuse is 0.56 MB as a 1k JPG and 1.95 MB as a 2k JPG. One PBR set at 2k is roughly 4 maps × about 2 MB. Expect a few dozen materials at 2k, or a hundred or more at 1k, before KTX2 compression changes the numbers. A 2k HDR sky is 6.7 MB and a 1k is 1.7 MB (`moonless_golf`). Treat these as order-of-magnitude figures only. Measuring the KTX2 output belongs to the pipeline ticket.

## By asset kind

### Environment PBR textures: covered

- **Poly Haven** has 869 textures. The categories that matter here: *dirty* 117, *brick* 108, *plaster-concrete* 136, *wood* 138, *metal* 25, *roofing* 23, *cobblestone* 38, *asphalt* 20.
- The metal set alone covers a "corrugated works" Zone: `corrugated_iron` (three variants), `rusty_corrugated_iron`, `worn_corrugated_iron`, `rusty_metal` (five variants), `rusted_shutter`, `metal_grate_rusty`, `factory_wall` and `container_side`.
- Source: <https://api.polyhaven.com/assets?type=textures>, plus `&categories=metal`.
- **ambientCG** has 2,041 materials. The largest families: Tiles 164, PavingStones 156, Bricks 132, Ground 120, Wood 102, Metal 101, Planks 58, Concrete 62, Facade 26, PaintedPlaster 18, PaintedMetal 18, PaintedWood 17, CorrugatedSteel 15, DiamondPlate 21, Rust 10, plus SurfaceImperfections 20 and Smear 8 for grime overlays.
- Source: <https://ambientcg.com/api/v2/full_json?type=Material>.
- **Fit:** both libraries are photo-scanned and photoreal, which is exactly the textured-greybox grit wanted. Each Zone material set (plaster house, brick yard, corrugated works) can come from a single library, so it looks consistent.

### Trim and decals: partly covered

- **ambientCG Decals (127).** Leaking stains (many), road lines, manhole covers, asphalt damage, tyre tracks, paving edge, and two door decals.
- **ambientCG Atlases (61).** GraffitiSet, CardboardSet, Tape, AsphaltDamageSet, ScrewSet, LeafSet and Foliage.
- Source: <https://ambientcg.com/api/v2/full_json?type=Decal>, `type=Atlas`.
- Poly Haven has no decal category.
- **Gaps:** blood splatter and smears, bullet holes, scorch marks, period posters and signage, and wall-buy chalk outlines. No credible CC0 library source was found for these. Author them (Blender bake or hand-painted alpha) or test OGA CC0 one asset at a time.
- **Trim sheets:** none of these libraries ships a trim sheet as such. Cut trims from tileable materials (planks, metal plate, brick) offline in Blender.

### Props: covered

- **Poly Haven** has 521 models: *props* 176, *industrial* 97, *furniture* 85, *containers* 68, *tools* 70, *lighting* 29.
- Matching industrial pieces: `Barrel_01/02/03`, `barrel_stove`, `old_military_crate`, `ammo_box`, `concrete_road_barrier`, `modular_chainlink_fence`, `modular_industrial_pipes_01`, `modular_electric_cables`, `modular_electricity_poles`, `rollershutter_door`, `portable_generator`, `old_military_compressor`, `power_box_01`, `metal_tool_chest`, `steel_frame_shelves`, `street_lamp_01/02`, `caged_hanging_light`, `industrial_pipe_lamp`, `old_gas_mask`, `wooden_ladder`, `old_tyre`, `fire_hydrant`, `water_manhole_cover`.
- Source: <https://api.polyhaven.com/assets?type=models>.
- **Fit:** realistic, worn and consistent in scale. Kenney and Quaternius props are stylised and clash with this look, so keep them out of view of the realistic set.

### Interactables: gap, kitbash

- No CC0 library has anything like a Mystery Box, Pack-a-Punch, Perk machine (vending machine) or boarded window.
- Plan to kitbash:
  - Box: `old_military_crate` plus a wood or metal material.
  - Pack-a-Punch: `old_military_compressor`, `power_box_01` or `metal_tool_chest` with an emissive accent.
  - Perks: a box body with ambientCG PaintedMetal, a decal logo and an emissive panel. Logos must be authored, since perk branding doesn't exist as CC0.
  - Barriers: ambientCG Planks on the existing geometry.
  - Doors: `rollershutter_door` or ambientCG Door decals.
- Poly Haven's 15 *rigged* models (e.g. `power_box_01`, `metal_tool_chest`, `bench_vice_01`) have hinged parts usable for open and close animations. Source: same API, `categories` contains `rigged`.

### Weapons and first-person arms: mostly a gap

- **Poly Haven.** `bolt_action_rifle_7_62` is a realistic Mosin-type rifle with 1k–8k textures, shipped as glTF, FBX, USD and .blend. There is also `service_pistol`, plus props (`ammo_box`, `old_military_crate`) and melee (`machete`, `hatchet`).
- **Sketchfab CC0** (`license=cc0`, downloadable):
  - Museum scans of real firearms from the user "ArmsMuseum": "Infantry Rifle mod. 1891 with Bayonet" (27k faces) and "Special Sniper Rifle VSS Vintorez".
  - From "WirtualneMuzeaMalopolski": "Vis P35(p) pistol" (64k faces) and a shotgun (326k faces).
  - These are period-correct but scans, not game-ready. They need retopology, a split for moving parts, and a texture rebake.
  - Low-poly CC0 guns from "treehero" (180–420 faces, animated) are stylised.
  - Source: <https://api.sketchfab.com/v3/search?type=models&license=cc0&downloadable=true&q=rifle> (also `pistol`, `shotgun`).
- **Quaternius** Ultimate Guns Pack (40 models) and Animated Guns Pack (6) are low-poly stylised. Same Quaternius licence caveat.
- **Arms.** OpenGameArt CC0 has "FPS arms (rigged only)", which its author calls a "crude first attempt" (<https://opengameart.org/content/fps-arms-rigged-only>), and "Low poly weapons pack with rigged arms" (<https://opengameart.org/content/low-poly-weapons-pack-with-rigged-arms>). Both are stylised.
- **Gap:** there is no CC0 set of WaW/BO1-era guns (Thompson, MP40, STG-44, M1 Garand, Trench gun, Ray Gun) that are game-ready, share a style, and come with matching arms. Expect to model or kitbash these in Blender, using the Poly Haven rifle and pistol as the quality reference.

### Rigged and animated zombies: animation covered, meshes a gap

- **Quaternius:**
  - Animated Zombie Pack: 2 models, atlas-textured, many animations.
  - Zombie Apocalypse Kit: 60 models, "4 Playable characters, 20 animations each, 4 enemies, and best of all 2 DOGS".
  - Universal Animation Library 2: 130+ animations on a universal humanoid rig, "zombie locomotion" included, FBX/GLB/.blend, and also mirrored on OGA as CC0.
  - The meshes are low-poly and stylised, which suits the animations more than the look.
- **OpenGameArt CC0 zombies:** "Male City Zombie" (<https://opengameart.org/content/male-city-zombie-ready-for-use-in-game-engines>), "Mobile ready zombie" (1,938 tris, 512 texture, <https://opengameart.org/content/mobile-ready-zombie>) and "Hazmat Zombie" (provenance caveat above). Quality and rigging vary, so check each one.
- **Sketchfab CC0:** the search for "zombie" returned no downloadable CC0 results.
- **Gap:** no CC0 zombie is both realistic and properly rigged. The workable path is to retarget UAL2's zombie locomotion, attacks and deaths in Blender onto the best OGA CC0 zombie, or onto a semi-realistic Quaternius base character (Universal Base Characters, "Regular" proportions) roughened with grime textures.

### Dogs (Hellhounds) and specials: gap

- The only animated CC0 dogs found are in the Quaternius Zombie Apocalypse Kit (2 dogs, stylised) and possibly the Ultimate Animated Animal Pack (12 animals, each with 12+ animations: attack, death, gallop, walk).
- Sketchfab CC0 "dog" results are museum sculptures, not usable.
- There is no CC0 source for the specials (Sapper, Shrieker, Nova and Napalm equivalents). They will be kitbashed variants of the zombie mesh with emissive, particle or material changes.

### Sky HDRIs: covered

- **Poly Haven** has 997 HDRIs: 61 *night*, 225 *sunrise-sunset*, 134 *overcast*, 499 *urban*. Night examples: `moonless_golf`, `moonlit_golf`, `cobblestone_street_night`, `dresden_station_night`, `kloppenheim_02/07` (with `_puresky` variants), `courtyard_night`.
- **ambientCG** has 437 HDRIs, including NightSkyHDRI and EveningSkyHDRI families.
- Sources: <https://api.polyhaven.com/assets?type=hdris>, <https://ambientcg.com/api/v2/full_json?type=HDRI>.

### Audio: SFX covered, music and announcer a gap

`ls assets/audio` lists 113 slot files plus 8 announcer lines in `assets/audio/announcer/`.

**Freesound CC0 depth** (search with the CC0 facet on 2026-11-12, before rate-limiting stopped it): "zombie" 928, "gunshot" 894, "gun reload" 398, "wood break" 247, "dog growl" 234, "vending machine" 110. Kenney CC0 audio includes Impact Sounds (130 files) and a Voiceover Pack (90 files).

| Slot group | Slots | Source | Coverage |
|---|---|---|---|
| Gunfire and handling | `gunshot_{pistol,smg,rifle,shotgun,lmg,sniper,launcher}`, `reload`, `empty_click`, `weapon_swap`, `knife_swing`, `knife_hit`, `claymore_plant`, `explosion`, `flamethrower` | Freesound CC0, Kenney Impact | Good |
| Zombie vocals | `zombie_{walker,jogger,sprinter}_idle`, `zombie_attack`, `zombie_death`, `zombie_riser`, `crawler_vocal`, `crawler_drag`, `crawler_gib`, `crawl_drag`, `helmet_off` | Freesound CC0 ("zombie" 928) | Good, with editing into variants |
| Hellhounds | `hound_{idle,bite,death,spawn}`, `hellhound_round_start` | Freesound CC0 ("dog growl" 234) plus thunder or electric layers | Good for SFX. `hellhound_round_start` is a stinger, see music |
| Specials | `sapper_*` (8), `shrieker_*` (5), `nova_*` (5), `napalm_*` (5), `scream_hit`, `scream_pop` | Freesound CC0 monster, fire, gas and metal, layered | Partial: needs sound design, not a straight pick |
| Player and foley | `footstep`, `dive_land`, `stance_rustle`, `player_hurt`, `down`, `self_revive`, `low_health`, `shellshock`, `hit`, `headshot`, `kill` | Freesound CC0, Kenney Impact | Good |
| World and machines | `door_open`, `plank_tear`, `plank_rebuild`, `box_move`, `box_teddy`, `perk_drink`, `perk_machine_leave`, `pap_process`, `pap_ready`, `pap_shot_layer`, `power_on`, `power_down`, `power_restore`, `trap_active`, `trap_kill`, `teleporter_{charge,fire,arrive}`, `mainframe_link`, `fire_burn`, `fire_pool` | Freesound CC0 ("wood break" 247, "vending machine" 110) | Good to partial |
| Wonder weapons and ammo mods | `ray_gun`, `thundergun`, `monkey_bomb`, `nuke_blast`, `ammo_mod_*` (5), `napalm_explode` | Freesound CC0 sci-fi and electric, layered | Partial: sound design |
| Power-ups and UI | `powerup_hum`, `powerup_pickup`, `buy`, `cant_afford`, `offer_pick`, `boon_offer`, `pact_offer`, `fire_sale` | Kenney audio packs, Freesound CC0 | Good |
| **Music and stingers** | `perk_jingle_{juggernog,quick_revive,speed_cola,double_tap}`, `box_spin`, `round_start`, `round_end`, `game_over`, `hellhound_round_start` | None: these are bespoke tunes | **Gap.** Compose them, or keep the synthesised versions |
| **Announcer** | `announcer/announcer_{max_ammo,insta_kill,double_points,nuke,carpenter,fire_sale,hellhound_round}`, `sapper_warning` | None found with these exact phrases. Kenney Voiceover Pack is generic words (not checked line by line) | **Gap.** Record or synthesise them. The current placeholders use espeak-ng (`assets/audio/README.md`) |

Variant support: the existing `_1.._n` variant scheme (`assets/audio/README.md`) suits Freesound well, since packs there often ship 3–8 takes of one sound.

## Gaps (no credible CC0 source)

1. WaW/BO1-era **game-ready guns** with a shared style, plus **first-person arms**.
2. **Realistic rigged zombie meshes.** Animation exists (Quaternius UAL2); fitting meshes don't.
3. **Realistic dogs** and every **special** enemy.
4. **Interactable hero pieces**: Box, Pack-a-Punch, Perk machines and their logos.
5. **Gore and combat decals**: blood, bullet holes, scorch. **Period signage and posters.**
6. **Music**: perk jingles, round stingers, box spin tune, game-over.
7. **Announcer voice lines.**

## Not pursued (leads only)

- The Smithsonian Open Access API returns firearm records (e.g. "Remington Rolling Block Rifle"), but its policy page returned 403 to automated fetches, so whether its 3D models are CC0 was not checked.
- KayKit, Poly Pizza and similar aggregators were not checked. Poly Pizza mixes CC0 and CC-BY, and KayKit is stylised in any case.
- Mixamo, Sonniss GDC bundles and Synty are generally understood to be royalty-free but not CC0. That was not re-checked here. They are out under the brief.
