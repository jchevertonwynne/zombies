# walker

Makes the skinned Walker the game draws a plain Zombie with (`assets/enemies/walker/`), reproducibly,
in headless Blender. `./build.sh` fetches every source and runs it all; it uses a throwaway Blender
profile (`BLENDER_USER_RESOURCES`), so your own Blender is left alone.

- `build_walker.py`: a MakeHuman body (MPFB) in the official CC0 sweater and wool trousers, brought
  into the UAL rig's T pose, skinned to UAL2's skeleton with the weights of UAL2's own mannequin,
  split into `Walker_Head`, `Walker_LegL/R`, `Walker_PantsLegL/R` and the rest, and exported with
  UAL2's `Zombie_Walk_Fwd_Loop` and `Zombie_Idle_Loop`. The rig is UAL2's own armature, so the clips
  need no retargeting. No Quaternius mesh is used, only its skeleton, weights and clips.
- `make_textures.py`: decayed skin and stained clothes maps from the official CC0 MakeHuman maps,
  as PNG for `tools/texpack --color --opacity` (which writes the KTX2 the game loads).

Provenance is in `assets/manifest.json`. The game dresses the parts itself by material name
(`WalkerSkin`, `WalkerShirt`, `WalkerPants`), so the glTF holds no textures.
