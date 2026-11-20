# audio

`process.py` turns a CC0 recording into one variant file of an audio slot and records its
provenance in `assets/manifest.json`. Run `python3 tools/audio/process.py --help`, or read the
docstring at the top of the script. It needs `ffmpeg` with libvorbis and nothing else.

```
python3 tools/audio/process.py step.wav footstep_wood --group footsteps \
    --source https://freesound.org/people/x/sounds/123/ --licence CC0 --start 0.12 --end 0.55
```

Per-floor footsteps are the optional slots `footstep_wood`, `footstep_tile`, `footstep_brick`,
`footstep_concrete`, `footstep_metal`, `footstep_cobble` and `footstep_dirt`. A teammate's step
picks the slot of the floor family under their feet (each floor `Material` declares its family in
`src/maps/map/textures.rs`), and falls back to `footstep` when that slot has no files. Source
recordings are never committed.
