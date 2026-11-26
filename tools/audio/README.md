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

`compose.py` writes the jingles and stingers: the round stingers, Game Over, the Gauntlet's cues,
the Box's spin tune and Teddy, and the four Perk jingles. They are our own pieces, released as CC0,
synthesised from scratch in plain Python (no samples, no packages beyond the standard library),
given a room and some tape wear with ffmpeg, then handed to `process.py` with the licence
`CC0 (composed for this project)` and the script as their source.

```
python3 tools/audio/compose.py --list            # the pieces
python3 tools/audio/compose.py                   # write every one into assets/audio
python3 tools/audio/compose.py round_start --render-only --keep /tmp/wav   # just listen
```

A piece marked `layer=True` (`quarantine_release_chime`) is not a slot: render it with `--render-only --keep DIR`
and mix it under its recording with ffmpeg before handing the mix to `process.py` (the Quarantine's release
cue is the airlock hiss with that chime on it).

`archive_sounds.py` builds the Archive's twenty files (#542) from CC0 Freesound previews in a folder of `<id>.mp3`s: it cuts, loops
(equal-power cross-fade), layers and, for the Range's thud, convolves with a cathedral impulse response (`afir`), then hands each slot to
`process.py`. Its docstring lists the sources and where it departs from the look decision (no recording of a shelf on rails exists, and
Kenney's sounds have no direct download).
