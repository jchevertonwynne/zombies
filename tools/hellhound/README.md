# hellhound

Builds the Hellhound's model: rigid parts (Torso, Head, Jaw, Tail, four legs, two eyes, ears) kitbashed in headless Blender, with a CC0 hide as KTX2.

```
python3 tools/hellhound/build.py            # Blender on the PATH, Rust, and the network
python3 tools/hellhound/build.py --hide brown_leather --blender /Applications/Blender.app/Contents/MacOS/Blender
```

It writes `assets/models/hellhound/` (`hellhound.gltf`, `hellhound.bin`, and the hide's `color`, `normal` and `orm` KTX2) and one manifest entry for the directory.

The game finds the parts by node name, so keep them: `Torso`, `Head`, `Jaw`, `Tail`, `LegFL`, `LegFR`, `LegBL`, `LegBR`, `EyeL`, `EyeR`. Each part's origin is its joint. `Head` and the four legs are required; a model missing one stays the box hound. The animation is all in code (`src/enemies/hellhound_rig.rs`), so the glTF holds no clips.

The model never replaces the hit areas, and without the files the game draws today's box hound, which the same rig animates too. Headless tests never load it.
