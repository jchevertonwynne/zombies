# zombie

Makes the skinned bodies the game draws a plain Zombie and a Crawler with (`assets/enemies/zombie/`),
reproducibly, in headless Blender. `./build.sh` fetches every source and runs it all; it uses a
throwaway Blender profile (`BLENDER_USER_RESOURCES`), so your own Blender is left alone.

- `build_bodies.py --variant labourer|soldier|civilian`: a MakeHuman body (MPFB), shaped by the
  variant and dressed in official CC0 clothes (a sweater and wool trousers, a tucked tee and cargo
  trousers, a polo and jean shorts), brought into the UAL rig's T pose, skinned to UAL2's skeleton
  with the weights of UAL2's own mannequin, split into `<Name>_Head`, `<Name>_LegL/R`,
  `<Name>_PantsLegL/R` and the rest, and exported with only `Zombie_Idle_Loop`. The cuts the split
  leaves (the neck, the thighs) are capped, with UVs on a dark-red patch in the corner of each map.
  The rig is UAL2's own armature, so every clip fits every body. No Quaternius mesh is used, only its
  skeleton, weights and clips.
- `build_clips.py`: the one clips glTF, a bare skeleton. UAL2's `Zombie_Idle_Loop`,
  `Zombie_Walk_Fwd_Loop` and `Zombie_Scratch` as they are; UAL1's (the OpenGameArt mirror, a
  different `DEF-*` skeleton) `Jog_Fwd_Loop`, `Sprint_Loop`, `Death01` and `Hit_Chest` retargeted by
  world-space rotation. It prints each locomotion clip's `FOOT SPEED`, the ground speed it is
  authored for (the game's `WALK_SPEED`, `JOG_SPEED` and `SPRINT_SPEED`).
- `make_textures.py`: decayed skin (shared by every body) and each body's stained shirt and soiled
  trousers, from the official CC0 MakeHuman maps, as PNG for `tools/texpack --color --opacity`
  (which writes the KTX2 the game loads). The soldier's and civilian's are half size.

A second colourway of each body is a colour multiplier on the same maps (`TINTS` in
`src/enemies/zombie/skin.rs`), so it costs no files.

Provenance is in `assets/manifest.json`. The game dresses the parts itself by name (`...Shirt`,
`...Pants`, anything else is skin), so the glTFs hold no textures.
