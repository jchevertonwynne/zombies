# UAL2 clip coverage for our enemies

Research for [#297](https://github.com/jchevertonwynne/zombies/issues/297), part of the map [#272](https://github.com/jchevertonwynne/zombies/issues/272). [#280](https://github.com/jchevertonwynne/zombies/issues/280) settled that enemy animation comes only from the OpenGameArt (OGA) mirror of Quaternius's Universal Animation Library 2 (UAL2). Does that mirror hold every clip our enemies need?

**Short answer: no.** The OGA mirror is the free **Standard** cut, with **43 clips**, and only three are zombie clips: `Zombie_Idle_Loop`, `Zombie_Walk_Fwd_Loop` and `Zombie_Scratch`. That covers the Walker, idling and (roughly) the swing and the Barrier clawing. There is **no run, no sprint, no crawl and no death** clip, and **no quadruped at all**, so the Hellhound isn't covered. Two other Quaternius uploads on OGA pass #280's proof test and fill some gaps: **UAL1's OGA mirror** (`Death01`, `Jog_Fwd_Loop`, `Sprint_Loop`, hit flinches, punches) and the **Farm Animal Pack's Pug** (a dog with `Run`, `Walk`, `Jump`, `Death`, `Idle`). Crawling, the Shrieker's scream, the Hellhound's bite and rising out of the ground have no provably-CC0 clip anywhere I found.

## Sources

Primary, all fetched 2026-11-13:

1. **The UAL2 OGA page**, [opengameart.org/content/universal-animation-library-2](https://opengameart.org/content/universal-animation-library-2), and its only file, [`universal_animation_library_2standard.zip`](https://opengameart.org/sites/default/files/universal_animation_library_2standard.zip) (10.3 MB). I downloaded it and read the clip list straight out of the glTF JSON in `Unreal-Godot/UAL2_Standard.glb`.
2. **Wayback Machine copies** of that page ([2026-01-24](http://web.archive.org/web/20260124011231/https://opengameart.org/content/universal-animation-library-2), [2026-02-18](http://web.archive.org/web/20260218061033/https://opengameart.org/content/universal-animation-library-2)) and of the zip ([2026-01-24](https://web.archive.org/web/20260124011129/https://opengameart.org/sites/default/files/universal_animation_library_2standard.zip)).
3. **The UAL1 OGA page**, [opengameart.org/content/universal-animation-library](https://opengameart.org/content/universal-animation-library), its zip, and Wayback copies of the zip.
4. **Quaternius's quadruped packs on OGA**: [Animated Animales Low Poly](https://opengameart.org/content/animated-animales-low-poly) (Animal Pack Vol.2: Wolf, Dog, ...) and [LowPoly Animated Farm Animal Pack](https://opengameart.org/content/lowpoly-animated-farm-animal-pack) (Pug, ...). I read the action names out of the `.blend` and `.fbx` files.
5. **Other CC0 quadruped candidates on OGA**, from OGA's advanced search (3D Art, CC0, "wolf" and "dog animated"). Each page and, where needed, its files are cited in the table below.
6. **Our code**: `src/enemies/zombie/rig.rs` (`Activity`, `pose`, `crawl`), `src/enemies/zombie/mod.rs`, `hellhound.rs`, `nova.rs`, `special.rs`, `sapper.rs` on `main` at `1aee470`.
7. Quaternius's own [UAL2 pack page](https://quaternius.com/packs/universalanimationlibrary2.html). I used it only to confirm the Standard/Source split. Under #280 it isn't proof of a licence.

## 1. Is the OGA upload Quaternius's own, CC0, and dated before 2026-08-28?

**Yes, on all three counts.**

- **Uploader.** The OGA author is the account [`quaternius`](https://opengameart.org/users/quaternius) (user 32222). Its profile links to `http://quaternius.com` and lists Quaternius's whole catalogue (UAL1 and UAL2, the MegaKits, the 2017 animal packs). The zip's own `License.txt` says "Models by @Quaternius" and links Quaternius's Patreon.
- **Licence.** The OGA page's License field reads **CC0**. That was already true in the first Wayback copy, from 2026-01-24. The bundled `License.txt` reads "CC0 1.0 Universal (CC0 1.0) Public Domain Dedication". The page also sits in OGA's "All CC0 - Uploader: quaternius" collection.
- **Date.** The page is stamped **Friday, January 23, 2026 - 10:43**. The zip's entries are dated 2026-01-18 to 2026-01-23.
- **Unchanged since.** The zip I downloaded today has SHA-256 `ec0e40d6d78fe9aaad59e322f40865a8675c22f0745e291622e54520391a9217`. That is byte-identical to the Wayback capture of 2026-01-24: the same SHA-256, and SHA-1 (base32) `XJVYS3N3AYVCJ45WW7I2ZEKAF3TZKAHB`, which matches the CDX digest. So what OGA serves now is exactly what was published under CC0 seven months before the QAL. **This is the archive link the provenance manifest (#280 point 4) should record.**

**One catch: it's the Standard cut.** The page says the full kit has "130+ animations". Quaternius's pack page says the free version is "60-70%" of the kit, and that the Source version (all clips plus the `.blend`) is sold on quaternius.com. The OGA zip holds **43 clips**. The rest exist only in the paid Source version. Under #280 that version isn't provably CC0, so it's out.

## 2. What the UAL2 OGA mirror holds

These are the 43 clips in `UAL2_Standard.glb`, with lengths from the glTF animation-sampler input maxima. All are humanoid clips on one rig, which also ships with a male mannequin mesh. A female mannequin `.glb`/`.fbx`/`.blend` with no animations is included as well. `_RM` means the clip carries root motion.

| Group | Clips |
|---|---|
| **Zombie** | `Zombie_Idle_Loop` 1.33 s, `Zombie_Walk_Fwd_Loop` 1.33 s, `Zombie_Scratch` 1.80 s |
| Hits / falls | `Hit_Knockback` 0.83 s, `Hit_Knockback_RM` 0.83 s, `LayToIdle` 1.53 s |
| Traversal | `ClimbUp_1m_RM` 0.67 s, `Slide_Start` / `Slide_Loop` / `Slide_Exit`, `NinjaJump_Start` / `NinjaJump_Idle_Loop` / `NinjaJump_Land` |
| Melee | `Melee_Hook` 0.47 s, `Melee_Hook_Rec`, `Sword_Regular_A` / `_A_Rec` / `B` / `_B_Rec` / `C` / `Combo`, `Sword_Block`, `Sword_Dash_RM`, `OverhandThrow` 1.33 s |
| Shield | `Idle_Shield_Loop`, `Idle_Shield_Break`, `Shield_OneShot`, `Shield_Dash_RM` |
| Work / props | `TreeChopping_Loop` 0.97 s, `Farm_Harvest`, `Farm_PlantSeed`, `Farm_Watering`, `Chest_Open`, `Consume`, `Walk_Carry_Loop` |
| Idles / emotes | `A_TPose`, `Idle_FoldArms_Loop`, `Idle_Lantern_Loop`, `Idle_No_Loop`, `Idle_Rail_Call`, `Idle_Rail_Loop`, `Idle_TalkingPhone_Loop`, `Yes` |

What isn't there matters as much. There is no generic walk, jog, run or sprint, no death, no crawl and no scream. UAL2 is meant to *complement* UAL1 (the page says so), so the basics live in UAL1.

I haven't looked at the clips in motion. Fit judgements below go by clip name and length. Each "partial" needs someone to look at it in Blender or Quaternius's viewer before it's relied on.

## 3. What our enemies animate today

Every state below comes from `src/enemies`. The standing Zombie rig is posed procedurally from an `Activity` (`rig.rs`). The Crawler rig poses only its arms (`crawl`). **Hellhounds and Nova Crawlers have no rig at all**: their bodies are static boxes (`hellhound.rs` `spawn_body`, `nova.rs` `spawn_body`), so their states are behaviours with no animation yet.

| Enemy | States (from code) |
|---|---|
| Zombie (standing rig) | `Moving` across three Speed Tiers (`Walker`, `Jogger`, `Sprinter`; the pose blends from arms-out reach to arm pumping by `run`), idling at zero speed, `Swinging` (one arm, alternating), `Tearing` (claw at a Plank, then yank it off), `Vaulting` (through the window), `Rising` (Risers clawing out of the ground), `Stunned`, `KnockedDown` (slide, lie, get up), `Dying` (arms flung up, falls backwards) |
| Crawler (legs blown off) | crawl (arm over arm), `Swinging` (one arm raised to swipe), `Dying` |
| Nova Crawler | crawl (low and quick), drop from a Vent (`VentDrop`) or rise at a Riser, death (bursts into gas if killed by a bullet). No rig today |
| Hellhound | lightning-strike arrival (`Telegraph`, stands invulnerable), run, bite, death. No rig today |
| Napalm | walk, rise, self-detonation wind-up (no pose of its own today), death = explosion |
| Shrieker | sprint, `Screaming`, death scream |
| Sapper | walk, comes through a window tearing every Plank in one blow, `Swinging { arm: Both }` (two-handed overhead slam), Sabotage work at a system (no pose of its own today), `Stunned` (Thundergun), `Dying`, helmet knocked off |

## 4. Coverage map

Key: **Yes** = a clip that fits. **Partial** = a clip that could stand in, or needs to be trimmed, layered or reused. **Gap** = nothing usable. The UAL1 and Pug columns are the extra sources from section 5.

| State | UAL2 OGA mirror | UAL1 OGA mirror | Verdict |
|---|---|---|---|
| Zombie idle | **Yes** `Zombie_Idle_Loop` | `Idle_Loop` | Covered |
| Zombie walk (Walker) | **Yes** `Zombie_Walk_Fwd_Loop` | `Walk_Loop` | Covered |
| Zombie run (Jogger) | Gap | Partial `Jog_Fwd_Loop` (a human jog) | Gap in UAL2. UAL1 needs a zombie arm-reach layer on top |
| Zombie sprint (Sprinter) | Gap | Partial `Sprint_Loop` (a human sprint) | Gap in UAL2. UAL1's pumping arms match today's sprinter pose |
| Swing at the Player | **Partial** `Zombie_Scratch`. Alternatives: `Melee_Hook`, `Sword_Regular_A/B` (one arm) | `Punch_Jab`, `Punch_Cross` | Covered by `Zombie_Scratch` if it reads as a swipe |
| Barrier tear (claw, then yank) | Partial: `Zombie_Scratch` for the clawing. Nothing for the yank (perhaps `Farm_Harvest`) | `Push_Loop`, `Interact` | Clawing covered, yank a gap |
| Vault through window | Partial `ClimbUp_1m_RM` | `Jump_Start` / `Jump_Land` | Partial |
| Rise out of the ground | Partial `LayToIdle` (getting up from lying) | `Crouch_Idle_Loop` | Gap (nothing claws up out of the earth) |
| Stunned | Partial `Hit_Knockback` | `Hit_Chest`, `Hit_Head` (short flinches) | Partial (flinches would need looping or holding) |
| Knocked down, then up | Partial `Hit_Knockback(_RM)` + `LayToIdle` | `Roll` | Partial. The get-up is covered |
| Death | **Gap** | **Yes** `Death01` 2.4 s | Gap in UAL2. UAL1 covers it |
| Crawler crawl / swipe / death | Gap | Gap (`Swim_Fwd_Loop` turned face-down is a speculative arm-over-arm hack) | **Gap** |
| Nova Crawler crawl | Gap | Gap | **Gap** |
| Nova Vent drop | Partial `NinjaJump_Land` | `Jump_Land` | Partial |
| Shrieker sprint | Gap | Partial `Sprint_Loop` | As for Sprinter |
| Shrieker scream | Gap | Gap | **Gap** |
| Napalm wind-up | Gap (perhaps `Idle_Shield_Break` or `Consume`, unverified) | Gap | Gap. Today it has no pose either |
| Sapper overhead slam | Partial `Sword_Regular_C`, `OverhandThrow` | Partial `Sword_Attack` | Partial |
| Sapper Sabotage work | Partial `TreeChopping_Loop` (hacking at something) | `Fixing_Kneeling`, `Interact` | Covered well enough |
| Sapper one-blow window entry | Partial `Zombie_Scratch` / `Melee_Hook` + `ClimbUp_1m_RM` | `Punch_Cross` | Partial |
| **Hellhound** run / bite / death / arrival | **Gap: UAL2 is humanoid-only** | Gap (humanoid-only) | **Not covered by UAL2.** See section 5 |

**Tally, against UAL2 alone:** 3 states are covered outright (idle, walk, and swing via `Zombie_Scratch`), about 9 are partial stand-ins, and the rest are gaps: jog, sprint, death, crawl ×2, scream, rise, Napalm wind-up and the whole Hellhound. Adding UAL1 closes **death, jog and sprint**, and improves the stun and the hits. Crawl, scream, rise and the hound's bite stay gaps.

## 5. Other provably-CC0 sources for the gaps

These pass #280's proof test: on OGA, uploaded by Quaternius, CC0, and archived before 2026-08-28.

1. **UAL1 OGA mirror**: [Universal Animation Library](https://opengameart.org/content/universal-animation-library), page dated **2025-04-16**, licence **CC0**, bundled `License.txt` CC0 1.0. It's the Standard cut, with **46 clips** (the page says "45"). The zip I downloaded today has SHA-1 (base32) `EZSI36ENOJH52JVKQW2WJFFXPONTMTMA` (SHA-256 `18ff1a72...a3ac2`). That matches Wayback captures from [2025-04-20](https://web.archive.org/web/20250420013641/https://opengameart.org/sites/default/files/universal_animation_librarystandard.zip) through 2026-10-29. **It uses the same rig as UAL2**, so its clips retarget with the same Blender mapping. Clips that matter here: `Death01`, `Jog_Fwd_Loop`, `Sprint_Loop`, `Walk_Loop`, `Hit_Chest`, `Hit_Head`, `Punch_Jab`, `Punch_Cross`, `Sword_Attack`, `Roll`, `Crouch_Fwd_Loop`, `Crouch_Idle_Loop`, `Jump_Start/Loop/Land`, `Push_Loop`, `Interact`, `Fixing_Kneeling`, `Swim_Fwd_Loop`. *Using it means widening #280's "UAL2 only" to "UAL1 + UAL2", which is a decision for the map, not for this ticket.*
2. **Quaternius's Pug, the only provably-CC0 animated dog with a run and a death**: [LowPoly Animated Farm Animal Pack](https://opengameart.org/content/lowpoly-animated-farm-animal-pack), page dated **2018-06-08**, CC0. The zip ([`Farm Animals by @Quaternius.zip`](https://opengameart.org/sites/default/files/Farm%20Animals%20by%20%40Quaternius.zip)) matches Wayback captures from **2021-11-28** (SHA-1 base32 `YIRPOOOCPMLGVC7IGELCDSJ2FLMO27OW`). `Pug.blend` holds the actions **`Death`, `Idle`, `Jump`, `Run`, `Walk`, `WalkSlow`**. (The FBX export carries fewer, so use the `.blend`.) It has **no bite**: `Jump` is the nearest lunge. It's also a pug: short legs and a stubby body. Retargeting it onto a lean hound skeleton is real Blender work, and the gait may read wrong. Its mesh is out under #280. The same pack also has a Horse, Cow and others with the same six actions.
3. **Quaternius's Wolf and Dog**: [Animated Animales Low Poly](https://opengameart.org/content/animated-animales-low-poly) (Animal Pack Vol.2), page dated **2017-11-03**, CC0, archived from 2019. `Wolf.blend` and `Dog.blend` hold only **`Idle` and `Walking`**: no run, no bite, no death. The right shape for a hound, but too few clips.

Rejected:

- **[Wolf- mutant 3D](https://opengameart.org/content/wolf-mutant-3d)** (Prjsj8guge, 2026-05-27, labelled CC0, with walk, swipe and jump attack). The FBX files use the `mixamorig:` skeleton, and the clips are named after Mixamo's "Mutant" set (`Mutant Walking`, `Mutant Swiping`, `Mutant Jump Attack`). They're Mixamo animations, so the uploader's CC0 label can't be trusted. It's also a biped. **Out.**
- **[3D wolf animation for game](https://opengameart.org/content/3d-wolf-animation-for-game)** (tomek, 2022). Its clip list is ideal (Idle, Walk, Run, Jump, Attack, Die), but it's **GPL 3.0 / GPL 2.0**, not CC0. **Out.**
- **Rigged but unanimated CC0 wolves**: [Wolf](https://opengameart.org/content/wolf-1) (Micket, 2013, "unwrapped and rigged"), [Wolf Low Poly (Rigged)](https://opengameart.org/content/wolf-low-poly-rigged) (crownjoshua, 2016), [3d wolf](https://opengameart.org/content/3d-wolf) (NewDLC, 2016, "rigged"). These are CC0 hound-shaped rigs with **no clips**. Useful only as a base to hand-key a hound in Blender, or to take the Pug's clips. [Fox and Shiba](https://opengameart.org/content/fox-and-shiba) and [Wolf](https://opengameart.org/content/wolf-2) (pixelmannen) are not rigged.

I found no provably-CC0 **crawl**, **scream** or **dog bite** clip. Those states need hand-keyed Blender animation, or they stay with today's procedural rig poses (`rig.rs` already has crawl, scream, rise and the Barrier yank).

## Open questions for the map

- **Widen #280's allow-list to UAL1's OGA mirror (and the Pug)?** Both pass the same proof test. Without UAL1 there's no death, jog or sprint clip.
- **Which states fall back to procedural or hand-keyed animation?** On the evidence above: crawl (Crawler and Nova Crawler), scream, rise, the Napalm wind-up and the hound's bite at least. Perhaps the whole hound too, if the retargeted Pug reads wrong.
- **Look at the "partial" clips in motion** before relying on any of them. These findings go by clip names and lengths only.
