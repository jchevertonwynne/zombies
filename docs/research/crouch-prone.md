# Crouch and prone in WaW and BO1 zombies (research for #213)

Status: **primary sources were largely unreachable.** The web search and fetch tools available returned no dvar dump, GSC dump or weapon-file dump for WaW or BO1. Most numbers below are therefore *recalled from the shared IW/Treyarch engine lineage and unverified*. Treat them as starting points to tune by feel, not as facts. Every claim is tagged.

Tags: **[V]** verified against a fetched source, **[R]** recalled from memory of engine defaults, not verified, **[I]** inferred, **[?]** not found.

## Sources actually read

- Call of Duty Wiki, Dive to Prone: https://callofduty.fandom.com/wiki/Dive_to_Prone (only search-snippet level content; a direct fetch returned HTTP 402).
- BO1 Reimagined change log (community mod, a patch list, not game data): https://github.com/Jbleezy/BO1-Reimagined
- Searches for `player_standingViewHeight` / `player_crouchViewHeight` / `player_proneViewHeight` found nothing from CoD sources.

## Findings

### Move speed per stance (multiplier of standing walk)

| Stance | Multiplier | Tag |
|---|---|---|
| Stand | 1.0 | by definition |
| Crouch | about 0.65 | [R] |
| Prone | about 0.15 to 0.30 (crawl, very slow) | [R], low confidence |
| Sprint | about 1.4 to 1.5 (stand only) | [R]; this game already has 9.0/6.5 = 1.38 |

- Weapon move-speed scalars multiply on top of stance [R]. Per-weapon values from BO1 Reimagined's patch list [V, but base-game-relative, not stance figures]: pistols 100%, SMG 110% to 105%, LMG 87.5% to 85%, sniper 95% to 90%, knife 110%.
- Backwards, strafe and sprint-strafe speed are set to 100% by BO1 Reimagined, implying the base game slowed them [V, indirect].
- Stamin-Up adds +10% move and sprint speed [V]. Whether it applies to crouch/prone [?].

In this game's terms (walk 6.5): crouch about 4.2, prone about 1.0 to 2.0 if the recalled values hold.

### View (eye) height per stance

Engine defaults [R]: stand 60, crouch 40, prone 11 world units (standing capsule about 70 units). As multipliers of standing eye height: crouch about 0.67, prone about 0.18. For this game (eye about 1.6 above the feet): crouch about 1.07, prone about 0.29. Not verified for either game; WaW and BO1 may override via script.

### Hip spread, recoil, sway

- Weapon files carry per-stance hip spread (`hipSpreadStandMin/Max`, `hipSpreadDuckedMin/Max`, `hipSpreadProneMin/Max`) [R]. A 2012 blog confirms the stand/ducked/prone trio exists in CoD1 weapon files, so the structure is long-lived [V for CoD1 only].
- Numeric multipliers for WaW/BO1 [?]. Inferred: crouch tightens hip spread (roughly 0.5x to 0.7x of standing), prone tightens it most (roughly 0.3x to 0.5x) [I].
- Recoil and sway reduced when crouched/prone [R], magnitude [?].

### Transition times

[?] Not found. Engine stance changes play a short animation of roughly 0.3 to 0.5 s with limited movement [R, low confidence].

### Controls

- Defaults [R]: console: B (or R3) tap to crouch, hold to go prone. PC: C crouch, Z prone, with hold/toggle options. Not verified for either game.

### Jump and sprint from each stance

- Sprint only from standing [R]; pressing sprint while crouched or prone stands you up [R].
- Jump from crouch/prone: stands up first or is blocked [R, low confidence].

### ADS while prone

ADS works while prone in both games [R]; prone gives the steadiest aim. No source [?].

### Actions prone blocks

- Hackables (Hacker perk) can be used while crouched or prone [V].
- Shangri-La spikes do not trigger on crouched, prone or downed players [V]. Ascension's Centrifuge can damage players diving or crouching in the air [V].
- Lean is disabled while downed [V].
- Buying, reviving, boards, reloading, melee while prone [?]. Recalled: buying and boards work crouched; melee from prone is restricted [R, low confidence].
- Entering prone while drinking a perk was a bug in the Shangri-La mud pit (fixed), implying perk-drinking normally blocks stance changes [V, indirect].

### Dive to prone (outside PhD Flopper)

- Exists in BO1 and Black Ops II, not WaW [V per CoD wiki]. Triggered by pressing and holding stance/prone while sprinting [V].
- Some WaW weapons ported into BO1 zombies lack dive animations and fall back to the normal prone animation [V per CoD wiki].
- Players can down each other by diving onto one another in Zombies [V per CoD wiki snippet].
- BO1 Reimagined patches: can dive again immediately, can move sooner after diving, shorter dive startup [V], implying the base game has a dive cooldown, a recovery lock and a startup delay (durations [?]).

### Do Zombies and Hellhounds hit a low Player differently?

[?] Nothing found. Inferred: melee attacks are range checks against the player's position with no stance-specific damage [I]. Decide by design rather than parity.

### WaW vs BO1 differences

- Dive to prone: BO1 only [V].
- Everything else numeric [?]. Same engine lineage, so base values are likely similar [I].

## Gaps and suggested next sources

1. A real dvar dump or config from WaW / BO1 PC (`dumpdvars`, or mod-tools raw files such as `maps/_zombiemode*.gsc`). Would settle view heights, speeds and spread.
2. Mod-tools weapon files (`hipSpread*`, `adsSpread*`, `moveSpeedScale`, `adsMoveSpeedScale`).
3. Zombies speedrun/technical docs for dive timing and cooldown.
