# PROTOTYPE: How should the Greybox be lit? (#277)

Throwaway branch, never merged. Four looks on the flat-coloured Greybox, switchable in game with `[` / `]`
(`src/look_prototype.rs`): `today`, `moonlit` (the research's "Moody A"), `dusk`, `lamplit`.

- `ZOMBIES_LOOK=<look>` picks one at start; with `ZOMBIES_BENCH=greybox` the worst-case bench measures it.
- `ZOMBIES_LOOK_TOUR=<dir>` shoots every look at five viewpoints and quits.
- Run with a scratch `HOME`, and `BEVY_ASSET_ROOT` pointing at a checkout with generated sounds.

Sheets: one per viewpoint, 2×2 (today, moonlit / dusk, lamplit). Bench numbers: `bench.txt`.

**Verdict: dusk.**
