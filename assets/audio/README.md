# Audio slots

Every sound the game plays is a slot declared in `src/audio/cues.rs`. A slot is either one file, `<slot>.ogg`, or numbered variants, `<slot>_1.ogg`, `<slot>_2.ogg`… with no gaps. The game picks variants at random and never plays the same one twice in a row. Announcer lines live in `announcer/`.

The game checks every slot at startup and exits with a list of any that are missing, have a gap, or have both a plain file and numbered variants.

The files committed here are synthesised placeholders. To replace one, drop in your recording under the same name (or as numbered variants, after deleting the placeholder). To write placeholders for any slots that have no files:

```sh
cargo gen-sounds
```

It never touches a slot that already has files. `--force` regenerates every placeholder and deletes the slot's other files. Announcer lines need `espeak-ng` on the `PATH`.
