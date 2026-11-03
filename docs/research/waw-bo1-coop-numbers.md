# WaW and BO1 co-op numbers

Research for [Research: WaW and BO1 co-op numbers](https://github.com/jchevertonwynne/zombies/issues/115), part of [Classic co-op: 2–4 Players online](https://github.com/jchevertonwynne/zombies/issues/112).

**Question:** What are the WaW and BO1 co-op rules and numbers? Zombie health and Horde size by Player count, the alive cap by Player count, bleed-out time, revive time with and without Quick Revive, Quick Revive's co-op price and behaviour, what a Down costs, how a bled-out Player respawns, when Game Over comes, how Points work per Player, and which Power-ups apply to every Player.

## Sources

Most numbers here come straight from the game scripts, which are the most reliable source.

- **WaW**: the GSC scripts Treyarch shipped with the World at War mod tools, mirrored at [SyndiShanX/COD-GSC-Source `WAW-GSC/maps`](https://github.com/SyndiShanX/COD-GSC-Source/tree/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps). The shared `_zombiemode.gsc` is the late (Shi No Numa / Der Riese) version; Nacht and Verrückt have their own copies (`_zombiemode_prototype.gsc`, `_zombiemode_asylum.gsc`), which use the same co-op rules. Links below are written `W:file#Lnn`.
- **BO1**: the raw file dump of Black Ops 1, [JTAG7371/T5-RawFile-Dump](https://github.com/JTAG7371/T5-RawFile-Dump/tree/b93297b23e497435c13585f1e18ca39213b8de27), including the `mp/zombiemode.csv` difficulty table. Links below are written `B:file#Lnn`.
- **Engine defaults** that no script overrides: [SwagSoftware/KisakBlack](https://github.com/SwagSoftware/KisakBlack) (a BO1 engine reimplementation) and the BO1 `toolconfig/perks.ddl`.
- **Call of Duty wiki** ([Power-Ups](https://callofduty.fandom.com/wiki/Power-Ups)): only for the WaW Carpenter, whose script is missing from the WaW mirror. Fandom blocked direct fetches, so this one comes from search-result excerpts.

Permalink bases:
- W = `https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/`
- B = `https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/`

**How the BO1 values are set.** In BO1, `set_zombie_var(name, default, is_float, column)` reads `mp/zombiemode.csv` first and only falls back to the default in the script ([B:maps/_zombiemode_utility.gsc#L2232-L2257](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode_utility.gsc#L2232-L2257)). The game runs `difficulty = 1`, so `column = 2`, which is the **MEDIUM** column ([B:maps/_zombiemode.gsc#L466-L467](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode.gsc#L466-L467)). For example, the script's `3000` fallback for starting Points never applies: the CSV says 500 ([B:mp/zombiemode.csv](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/mp/zombiemode.csv)). Every BO1 number below is the MEDIUM value.

## Summary table

| Rule | WaW | BO1 | Repo today (solo) |
|---|---|---|---|
| Zombie health | 150, +100 per Round to Round 9, then ×1.1 per Round. **The same for any Player count.** | Same formula, same for any Player count | Same (`ZOMBIE_HEALTH_*`) |
| Horde size | `24 + int((N−1) × 6 × m)`, then scaled ×0.2/0.4/0.6/0.8 in Rounds 1–4 | Solo: `24 + int(3 × m)`. Co-op: `24 + int((N−1) × 6 × m)`. Scaled ×0.25/0.3/0.5/0.7/0.9 in Rounds 1–5 | BO1 solo (`HORDE_*`) |
| `m` (multiplier) | `max(1, R/5)`, and from Round 10 also × `R × 0.15` | Same | Same |
| Alive cap (Zombies) | Stop spawning while more than 31 are alive, so **32** at once for any Player count | Same, **32** | `ALIVE_CAP = 24` |
| Hellhound Round size | 6 per Player for the first two Dog Rounds, then 8 per Player | Same | 6/8 (solo) |
| Hellhounds alive at once | 2 × the number of Players not Downed | Same | `HELLHOUND_ALIVE_CAP = 2` |
| Bleed-out time | **30 s** (engine default; no script sets it) | **45 s** (`_zombiemode.gsc` sets it) | n/a (solo Last Stand is 10 s, `LAST_STAND_SECS`) |
| Revive time | 3 s; 1.5 s if the **reviver** has Quick Revive | Same | n/a |
| Revive reach (trigger radius) | 40 units | 75 units | n/a |
| Quick Revive price (co-op) | 1500, and it needs Power | 1500, and it needs Power | 500 solo, no Power needed (BO1 solo) |
| Quick Revive solo | Nothing special. A solo Down is Game Over | 500, no Power, self-revive after 10 s, 3 buys | BO1 solo |
| Down cost: Perks | All lost | All lost | All lost |
| Down cost: Points | 5% of current Points (rounded up to 10) | 5%, and **the reviver gets that 5% back** | none |
| Down cost: Weapons | None: put away while Down, given back on revive | Same; grenades are also held back and returned | Same |
| Downed pistol | Best pistol held, else Colt; Clip + 2 Clips in reserve (Ray Gun: Clip only) | Best pistol held by a ranked list, else M1911; Clip + 2 Clips | Mustang & Sally, 1 Magazine + 2 more |
| Bleed-out cost | Bled-out Player loses 0%. **Every other Player loses 10%** | Same | n/a |
| Respawn | At the end of the Round, near the team, with the starting pistol (and knife). Points kept. Grenades come at the next Round start | Same; plus if Round > 6 and Points < 1500, Points are raised to 1500 (WaW does this only on Shi No Numa) | n/a |
| Game Over | When a hit would Down the last Player who is not already Downed or a spectator | Same (solo: when out of Quick Revive lives) | Down without Quick Revive |
| Starting Points | 500 each | 500 each (CSV, for 1–4 Players) | 500 |
| Kill Points | 50 + bonus (head 50, neck 20, torso 10, melee 80), **to the killer only** | Same | Same (`POINTS_*`) |
| Hit Points | 10 per damaging hit, to the shooter | Same | 10 |
| Power-ups for everyone | Insta-Kill, Double Points (world-wide), Max Ammo (all Players), Nuke (+400 each), Carpenter (+200 each, Der Riese) | Same, plus Fire Sale (world-wide); Max Ammo skips Downed Players | Same set (solo) |
| Power-ups for one Player | none | Death Machine (only the Player who picks it up) | n/a |
| Drop threshold | Total Points earned by all Players > `N × 500 + 2000`, then each gap × 1.14; 3% chance per kill anyway; at most 4 per Round | Same | Solo form of the same |

N = number of Players, R = Round number.

## Zombie health

Health does not depend on Player count in either game.

- **WaW** ([W:_zombiemode.gsc#L1407-L1416](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_zombiemode.gsc#L1407-L1416)): starts at `zombie_health_start = 150`. Each Round from 2 to 9 adds `zombie_health_increase = 100`. From Round 10, each Round adds `int(health × 0.1)` (`zombie_health_increase_percent = 10 / 100`) ([W:_zombiemode.gsc#L359-L361](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_zombiemode.gsc#L359-L361)). WaW adds to the previous Round's health instead of recomputing it.
- **BO1** ([B:maps/_zombiemode.gsc#L2772-L2787](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode.gsc#L2772-L2787)): the same numbers (CSV MEDIUM: 150 / 100 / 0.1), but recomputed from Round 1 each Round. In normal play the results match WaW's.

## Horde size by Player count

Both games compute `max`, the Round's Horde, in `round_spawning()`:

```
max = zombie_max_ai (24)
m   = max(1, R / 5);  if R >= 10: m *= R * 0.15
```

- **WaW** ([W:_zombiemode.gsc#L1003-L1031](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_zombiemode.gsc#L1003-L1031)): `max += int((N − 1) × zombie_ai_per_player (6) × m)`. Then `max` is scaled: the first Round ×0.2, Round 2 ×0.4, Round 3 ×0.6, Round 4 ×0.8. **WaW solo gets no bonus, so a solo Horde never grows past 24.** One exception: on Verrückt (`nazi_zombie_asylum`), Rounds 1–2 use `N × 3 + R` in co-op and 6 solo.
- **BO1** ([B:maps/_zombiemode.gsc#L2139-L2167](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode.gsc#L2139-L2167)): solo adds `int(0.5 × 6 × m)`; co-op adds `int((N − 1) × 6 × m)`, exactly as WaW. The early scaling is in `default_max_zombie_func` ([B:#L2091-L2116](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode.gsc#L2091-L2116)): ×0.25 / 0.3 / 0.5 / 0.7 / 0.9 for Rounds 1–5. A map can replace it with `level.max_zombie_func`.

From Round 6 on, the two games give **the same co-op Hordes**. They differ only in solo and in Rounds 1–5.

Computed from the formulas (health shown for comparison):

| Round | WaW 1P | WaW 2P | WaW 3P | WaW 4P | BO1 1P | BO1 2P | BO1 3P | BO1 4P | Health |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 4 | 6 | 7 | 8 | 6 | 7 | 9 | 10 | 150 |
| 2 | 9 | 12 | 14 | 16 | 8 | 9 | 10 | 12 | 250 |
| 3 | 14 | 18 | 21 | 25 | 13 | 15 | 18 | 21 | 350 |
| 4 | 19 | 24 | 28 | 33 | 18 | 21 | 25 | 29 | 450 |
| 5 | 24 | 30 | 36 | 42 | 24 | 27 | 32 | 37 | 550 |
| 6 | 24 | 31 | 38 | 45 | 27 | 31 | 38 | 45 | 650 |
| 10 | 24 | 42 | 60 | 78 | 33 | 42 | 60 | 78 | 1045 |
| 15 | 24 | 64 | 105 | 145 | 44 | 64 | 105 | 145 | 1679 |
| 20 | 24 | 96 | 168 | 240 | 60 | 96 | 168 | 240 | 2701 |
| 30 | 24 | 186 | 348 | 510 | 105 | 186 | 348 | 510 | 7000 |
| 50 | 24 | 474 | 924 | 1374 | 249 | 474 | 924 | 1374 | 47073 |

Two checks against well-known facts: WaW solo stays at 24 from Round 5, and BO1 solo Round 1 has 6 Zombies.

## Alive cap

Both games use the same spawn loop and stop spawning while `get_enemy_count() > 31` ([W:_zombiemode.gsc#L1036](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_zombiemode.gsc#L1036), [B:maps/_zombiemode.gsc#L2179](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode.gsc#L2179)). So **at most 32 can be alive at once, for any Player count**. The cap does not grow with Player count, so in co-op the Horde is just spread over more of the Round.

`zombie_max_ai = 24` is the Horde's base, **not** a cap on how many are alive (the CSV's `zombie_max_spawned = 6` is never read). The repo's `ALIVE_CAP = 24` matches the common community figure but not the script. A later ticket should choose between them on purpose.

**Hellhounds** ([W:_zombiemode_dogs.gsc#L91-L111](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_zombiemode_dogs.gsc#L91-L111), [B:maps/_zombiemode_ai_dogs.gsc#L100-L121](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode_ai_dogs.gsc#L100-L121)): a Dog Round has `N × 6` Hellhounds for the first two Dog Rounds and `N × 8` after that. The game spawns more only while fewer than `2 × (Players not Downed)` are alive. Hellhound health does not depend on Player count (400 / 900 / 1300 / 1600 by Dog Round, [B:#L300-L321](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode_ai_dogs.gsc#L300-L321)).

## Down, bleed-out and revive

### Bleed-out time

- **WaW: 30 s.** `_laststand.gsc` reads `player_lastStandBleedoutTime` ([W:_laststand.gsc#L116](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_laststand.gsc#L116)), and no WaW Zombies script sets it, so the engine default applies. That default is 30.0 in the shared engine family: KisakBlack registers it at 30.0 ([bg_misc.cpp](https://github.com/SwagSoftware/KisakBlack/blob/main/src/bgame/bg_misc.cpp)), and BO1's `toolconfig/perks.ddl` lists `player_lastStandBleedoutTime,30.0,0.0,60`. WaW multiplayer's Second Chance also lasts 30 s. *Confidence: high but not direct*, because I found no WaW engine dump. The COD5-Remastered mod sets 30 for its "classic" option and 45 otherwise, which fits this.
- **BO1: 45 s.** `SetDvar("player_lastStandBleedoutTime", "45")` ([B:maps/_zombiemode.gsc#L543](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode.gsc#L543)).
- In both games the vision fades to "death" over the second half of the timer. If someone is reviving the Player when it reaches 0, the bleed-out waits until the revive succeeds or stops ([W:_laststand.gsc#L277-L306](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_laststand.gsc#L277-L306)).
- A Downed Player is `ignoreme = true`: Zombies stop targeting them.

### Revive

- **3 s** in both games, **halved to 1.5 s if the reviver has Quick Revive**. The reviver's Perk is what counts, not the Downed Player's ([W:_laststand.gsc#L479-L483](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_laststand.gsc#L479-L483), [B:maps/_laststand.gsc#L636-L647](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_laststand.gsc#L636-L647); a BO1 comment says it "can no longer be tunable").
- The reviver holds Use inside a trigger radius around the Downed Player: **40** units in WaW ([W:_laststand.gsc#L35-L36](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_laststand.gsc#L35-L36)) and **75** in BO1 ([B:maps/_zombiemode.gsc#L542](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode.gsc#L542)).

### Quick Revive

- **WaW** ([W:_zombiemode_perks.gsc#L329-L337](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_zombiemode_perks.gsc#L329-L337)): **1500**, and like every Perk it needs Power. It does nothing for solo play; a solo Down is Game Over (see below).
- **BO1** ([B:maps/_zombiemode_perks.gsc#L768-L834](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode_perks.gsc#L768-L834)): co-op **1500**, waits for Power; solo **500**, no Power. The solo machine sells 3 times and then leaves ([B:#L1069-L1081](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode_perks.gsc#L1069-L1081)). In solo, the Last Stand pistol becomes the M1911 Upgraded (Mustang & Sally) and the self-revive comes after 10 s ([B:maps/_zombiemode.gsc#L3409-L3440](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode.gsc#L3409-L3440)). The repo already copies the solo rules.
- In co-op, Quick Revive in both games **only speeds up reviving others**. It gives no self-revive.

### What a Down costs

- **Perks: all of them.** Each Perk's thread waits for `player_downed` and then unsets the Perk ([W:_zombiemode_perks.gsc#L509-L511](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_zombiemode_perks.gsc#L509-L511), [B:maps/_zombiemode_perks.gsc#L1122-L1152](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode_perks.gsc#L1122-L1152)). A drink in progress is cancelled. Juggernog's loss resets max health to 100. BO1 also takes the Mule Kick third Weapon.
- **Points: 5%** of current Points, rounded up to the nearest 10 (`penalty_downed`) ([W:_zombiemode_score.gsc#L98-L135](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_zombiemode_score.gsc#L98-L135), [B:maps/_zombiemode_score.gsc#L187-L222](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode_score.gsc#L187-L222)). **BO1 gives that amount to the reviver** (`score_lost_when_downed` paid as a `"reviver"` award, [B:maps/_zombiemode.gsc#L1342-L1365](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode.gsc#L1342-L1365)). WaW pays nothing for a revive. BO1 takes the 5% on solo Downs too ([B:#L1408](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode.gsc#L1408)), but the repo's solo Down takes none: an existing departure.
- **Weapons: none.** They're stored when the Player goes Down and given back on revive, with the ammo they had ([W:_laststand.gsc#L141-L210](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_laststand.gsc#L141-L210)). BO1 keeps the inventory and just locks weapon switching ([B:maps/_laststand.gsc#L203-L300](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_laststand.gsc#L203-L300)), and holds back the grenades to return them on revive ([B:maps/_zombiemode.gsc#L1824-L1866](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode.gsc#L1824-L1866)).
- **Downed pistol.** WaW: the first pistol-class Weapon held, else the Colt; it gets a Clip plus 2 Clips in reserve, except the Ray Gun, which gets one Clip and no reserve ([W:_laststand.gsc#L236-L262](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_laststand.gsc#L236-L262)). BO1: the best pistol held, using a ranked list (M1911 < CZ75 < CZ75 DW < Python < upgraded ones < Ray Gun < Winter's Howl < upgraded Ray Gun / Winter's Howl), else the M1911. It gets a Clip plus 2 Clips, capped by the ammo the Player had, and the pistol's ammo is restored on revive ([B:maps/_zombiemode.gsc#L1556-L1650](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode.gsc#L1556-L1650)).

### Bleeding out and respawning

- When the timer runs out, the Player becomes a spectator (`zombify_player` → `spawnSpectator`; `zombify_player` is false in both) ([W:_zombiemode.gsc#L1471-L1477](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_zombiemode.gsc#L1471-L1477)).
- **Bleed-out penalty:** the bled-out Player loses `penalty_died = 0%`. **Every other Player loses `penalty_no_revive = 10%`** of their Points (`player_died_penalty`, [W:_zombiemode_score.gsc#L157-L164](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_zombiemode_score.gsc#L157-L164), [B:maps/_zombiemode_score.gsc#L272-L283](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode_score.gsc#L272-L283); BO1 CSV MEDIUM `penalty_no_revive,0.10`, `penalty_died,0.0`). That includes teammates who are Downed themselves.
- **When they come back:** at the end of the Round. `spectators_respawn()` runs after `round_wait()`, in the break between Rounds ([W:_zombiemode.gsc#L1336-L1371](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_zombiemode.gsc#L1336-L1371); BO1 only when `players.size != 1`, [B:maps/_zombiemode.gsc#L2717-L2721](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode.gsc#L2717-L2721)).
- **Where:** at a `player_respawn_point` struct near a living teammate (within 1000 units), using a slot by entity number. If there is none, at their own spawn point ([W:_zombiemode.gsc#L851-L930](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_zombiemode.gsc#L851-L930)).
- **With what:** the starting loadout: knife and starting pistol (WaW `zombie_colt`, [W:_loadout.gsc#L323-L329](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_loadout.gsc#L323-L329); BO1 `knife_zm` + `m1911_zm`, [B:maps/_loadout.gsc#L564-L572](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_loadout.gsc#L564-L572)). They have no Perks and keep their Points, less 0%. Grenades come from `award_grenades_for_survivors` at the next Round start: 2 if they have under a quarter of the max ([B:maps/_zombiemode.gsc#L2744-L2770](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode.gsc#L2744-L2770)). BO1 also takes their Claymores, Bowie/Sickle and abilities ([B:#L1918-L1977](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode.gsc#L1918-L1977)).
- **Points floor:** in BO1, a Player respawning after Round 6 with under 1500 Points is raised to **1500** on every map ([B:maps/_zombiemode.gsc#L1887-L1892](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode.gsc#L1887-L1892)). WaW does this only on Shi No Numa ([W:_zombiemode.gsc#L838-L842](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_zombiemode.gsc#L838-L842)).

### Game Over

`player_damage_override` counts the Players who are the victim, Downed, a spectator or zombified. If that count equals the number of Players (the hit would Down the last Player standing), the victim goes into a fake death and the game ends ([W:_zombiemode.gsc#L1737-L1769](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_zombiemode.gsc#L1737-L1769), [B:maps/_zombiemode.gsc#L3167-L3375](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode.gsc#L3167-L3375)). So:

- Game Over comes **as soon as every Player is Downed or dead at once**, even if some Downed Players still have bleed-out time left.
- A lone survivor keeps the game going while their teammates are spectators waiting for the Round to end.
- WaW solo: the first Down is Game Over. BO1 solo: Game Over comes when a Down happens with no Quick Revive lives left.

## Points per Player

Every Player has their own Points. There is no shared pool: BO1's `zombie_team_score_*` and `*_p_team` values are all 0 in the CSV and only matter in the versus prototype.

- **Start:** 500 each (WaW `zombie_score_start`, [W:_zombiemode.gsc#L365](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_zombiemode.gsc#L365); BO1 CSV `zombie_score_start_{1..4}p = 500`).
- **Kill:** 50 to the Player who lands the killing blow, plus a bonus by hit location: head 50, neck 20, torso 10, melee 80, burn 10 (both games, [W:_zombiemode_score.gsc#L11-L95](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_zombiemode_score.gsc#L11-L95), [B:maps/_zombiemode_score.gsc#L8-L120](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode_score.gsc#L8-L120)). BO1's `zombie_score_kill_{N}player` is 50 for every N, so kill pay does not drop with more Players. Earlier damage by teammates earns them nothing at the kill.
- **Hit:** 10 per damaging hit to the shooter (WaW `zombie_score_damage = 5`, rounded up to 10; BO1 `zombie_score_damage_normal = 10`).
- **Revive:** WaW pays nothing. BO1 pays the reviver the 5% the Downed Player lost.
- **Barrier rebuild:** per Player, capped each Round at `min(50 × R, 500)` (`rebuild_barrier_cap_per_round`, reset for every Player at Round start, [W:_zombiemode.gsc#L1337-L1343](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_zombiemode.gsc#L1337-L1343)).
- Every award is multiplied by the world-wide `zombie_point_scalar` (Double Points).

## Power-ups

[W:_zombiemode_powerups.gsc](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_zombiemode_powerups.gsc), [B:maps/_zombiemode_powerups.gsc](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode_powerups.gsc).

**Every Player benefits** from whichever Player picks it up:

- **Insta-Kill** (30 s) and **Double Points** (30 s): they set world-wide `level.zombie_vars`, so they apply to everyone ([W:#L501-L512](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_zombiemode_powerups.gsc#L501-L512), [W:#L475-L486](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_zombiemode_powerups.gsc#L475-L486)).
- **Max Ammo:** refills every Weapon of every Player. WaW includes Downed Players ([W:#L488-L499](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_zombiemode_powerups.gsc#L488-L499)); **BO1 skips Downed Players** ([B:#L1168-L1191](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode_powerups.gsc#L1168-L1191)).
- **Nuke:** **+400 to every Player** ([W:#L438-L444](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_zombiemode_powerups.gsc#L438-L444), [B:#L1124-L1128](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode_powerups.gsc#L1124-L1128)).
- **Carpenter:** **+200 to every Player** in BO1 ([B:#L945-L949](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode_powerups.gsc#L945-L949)). In WaW, Carpenter first appears on Der Riese (`include_powerup("carpenter")`, [W:nazi_zombie_factory.gsc#L440](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/nazi_zombie_factory.gsc#L440)). The shared powerups script in the mirror predates it; the CoD wiki says it also pays 200 to every Player (secondary source).
- **Fire Sale** (BO1 only, 30 s): world-wide.
- **Free Perk / Bottle** (BO1, on certain maps): a random Perk for every Player not Downed or spectating ([B:#L1542-L1552](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode_powerups.gsc#L1542-L1552)).

**Only the Player who picks it up:** the Death Machine (`minigun`) in BO1, and the dev-only `tesla` pickup ([B:#L783-L799](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode_powerups.gsc#L783-L799)).

**Drops:** the team shares one drop schedule ([W:#L141-L160](https://github.com/SyndiShanX/COD-GSC-Source/blob/8c8a1a843f5dc0d8a8ce3aadd8743b70bf0e9232/WAW-GSC/maps/_zombiemode_powerups.gsc#L141-L160), [B:#L366-L385](https://github.com/JTAG7371/T5-RawFile-Dump/blob/b93297b23e497435c13585f1e18ca39213b8de27/maps/_zombiemode_powerups.gsc#L366-L385)):

1. The game adds up every Player's `score_total` (all Points earned, including the 500 start).
2. A drop arms once that total passes `N × 500 + 2000`.
3. After that, each next threshold is the current total plus the last increment × 1.14.
4. Any kill also has a 3% chance to drop one (`randomint(100) <= 2`).
5. At most 4 drop per Round (`zombie_powerup_drop_max_per_round`).

The repo's solo values (`POWER_UP_THRESHOLD_START = 2000`, counting earned Points only) are the N = 1 case of this.

## Where WaW and BO1 differ (co-op)

| | WaW | BO1 |
|---|---|---|
| Bleed-out | 30 s | 45 s |
| Revive reach | 40 units | 75 units |
| Points for reviving | none | the 5% the Downed Player lost |
| Respawn Points floor | 1500 after Round 6, Shi No Numa only | 1500 after Round 6, every map |
| Early-Round Horde scaling | ×0.2/0.4/0.6/0.8 (Rounds 1–4) | ×0.25/0.3/0.5/0.7/0.9 (Rounds 1–5) |
| Solo Horde | no bonus (24 max) | +3 × m |
| Max Ammo | includes Downed Players | skips Downed Players |
| Downed pistol | first pistol held | best pistol held, by a ranked list |
| Grenades while Down | the pistol replaces them | held back and returned on revive |
| Solo Quick Revive | none (solo Down = Game Over) | 500, self-revive, 3 buys |

Same in both: zombie health, the co-op Horde formula (identical from Round 6), the 32 alive cap, Hellhound counts, revive time 3 s / 1.5 s, Quick Revive 1500 in co-op, losing every Perk on a Down, the 5% Down and 10% no-revive penalties, respawning at the end of the Round with the starting pistol, the Game Over rule, per-Player Points with 50-Point kills to the killer, and the world-wide Power-ups.

## Notes for later tickets

- The repo follows **BO1 solo** throughout (`HORDE_SOLO_BONUS`, `HORDE_EARLY_SCALE`, Quick Revive 500 / 3 buys, Mustang & Sally Last Stand), so BO1 is the natural co-op reference. The co-op Horde needs one new number, `zombie_ai_per_player = 6`, with the formula `(N − 1) × 6 × m` replacing `HORDE_SOLO_BONUS × m` when N > 1.
- `ALIVE_CAP = 24` (repo) and the scripts' 32 differ.
- The repo's solo Down takes no Points, but BO1 takes 5% even in solo. Co-op will need `penalty_downed = 0.05`, `penalty_no_revive = 0.10` and the BO1 reviver refund.
- "Bleed out" is still listed under _Avoid_ in `GLOSSARY.md`. These numbers are for that timer: 30 s in WaW, 45 s in BO1.
