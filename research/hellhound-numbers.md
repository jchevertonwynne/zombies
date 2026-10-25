# Hellhound (Dog) numbers and behaviour (WaW / BO1)

Context for issue #29. BO1 is the baseline; WaW differences are called out. Every number below comes from the shipped GSC unless it is marked otherwise.

## Sources

| Tag | Source | What it is |
|---|---|---|
| **[T5]** | [plutoniummod/t5-scripts](https://github.com/plutoniummod/t5-scripts) @ `70689b8`, under `ZM/` | BO1 stock zombies GSC. Dog logic: `Common/maps/_zombiemode_ai_dogs.gsc`. Round loop, mixed spawns: `Common/maps/_zombiemode.gsc`. Scoring: `Common/maps/_zombiemode_score.gsc`. Maps: `Maps/Der Riese/maps/zombie_cod5_factory.gsc`, `..._factory_teleporter.gsc`, `Maps/Kino der Toten/maps/zombie_theater.gsc`, `Maps/Shi No Numa/maps/zombie_cod5_sumpf.gsc`. |
| **[T4]** | [CallOfDutyModding/call_of_duty_world_at_war_mod_tools](https://github.com/CallOfDutyModding/call_of_duty_world_at_war_mod_tools) @ `b777df1`, under `raw/maps/` | WaW mod-tools scripts: `_zombiemode_dogs.gsc`, `_zombiemode.gsc`, `nazi_zombie_factory*.gsc`. |
| **[T5csv]** | Core-loop research (`research/core-loop-numbers.md`, from `BO1/zombiemode.csv`) | Point values. Not re-fetched here. |

The BO1 and WaW dog scripts are near-identical. Differences are listed in "WaW vs BO1" at the end. Locations are given as file plus function name, because line numbers drift between dumps.

The dump is for the BO1 stock maps. The shipped game may have been patched after the dump was taken; that cannot be checked from the scripts.

---

## 1. Dog Round scheduling

**First Dog Round** (`dog_round_tracker`, [T5] and [T4]):

```
level.next_dog_round = randomintrange(5, 8)      // 5, 6 or 7 (max is exclusive)
```

**Gap to the next one**, set when a Dog Round starts:

```
level.next_dog_round = level.round_number + randomintrange(4, 6)   // +4 or +5
```

- The check runs on `between_round_over`, after `level.round_number++` ([T5] `_zombiemode.gsc` ~L3979-3981). So `round_number == next_dog_round` means the Round that is about to start.
- The Round after a Dog Round (`dog_round_stop`) restores the normal spawn function and bumps `dog_round_count`.
- Dog Rounds are a plain Round-number schedule. **There is no solo vs co-op rule in the scheduling.** Player count only changes the dog count (section 2).
- Typical sequence: first on 5-7, then every 4 or 5 Rounds (for example 6, 10 or 11, 14-16, and so on).

**Which maps enable them:**

| Map | Evidence | Dog Rounds? |
|---|---|---|
| Der Riese (BO1) | `level.dogs_enabled = true;` then `maps\_zombiemode_ai_dogs::enable_dog_rounds();` | Yes |
| **Kino der Toten (BO1)** | same pair in `zombie_theater.gsc` (~L49, L88) | **Yes. Kino does have Dog Rounds in BO1.** |
| Shi No Numa (BO1) | same pair in `zombie_cod5_sumpf.gsc` | Yes |
| Ascension, Moon | `dog_spawn_func` set, but no `enable_dog_rounds()` call found | No Dog Rounds. Moon uses `special_dog_spawn()` in the no-man's-land code only |
| Verruckt, Five, Call of the Dead | `level.dogs_enabled = false;` | No |
| WaW Der Riese / Shi No Numa / Verruckt | `level.dogs_enabled = true;` in `nazi_zombie_factory/sumpf/asylum.gsc`; the WaW tracker starts from `init()` with no separate enable call | Yes (the mod-tools Verruckt copy also enables them) |

Nacht der Untoten (prototype) and Shangri-La were not found to enable them; Shangri-La and Moon were only grepped, not read.

**Interaction with other special Rounds** ([T5]):
- Kino's quad intro: `begin_quad_introduction` clears `dog_round` and pushes `next_dog_round` back one Round if it would land on the next Round. `time_for_quad_wave` skips a Round that is a Dog Round.
- Crawler Rounds (disabled by default; `level.crawlers_enabled` is commented out in `_zombiemode.gsc` and no stock map sets it): `next_crawler_round = next_dog_round + randomintrange(2, 3)`, which is always +2.

## 2. Dog count, cadence, spawn

From `dog_round_spawning` ([T5], [T4] identical):

```
wait(7)                                   // Round-start delay before the first spawn
max = players * 6      if dog_round_count < 3
max = players * 8      otherwise
level.zombie_total = max
```

- `dog_round_count` starts at 1 and goes up by one after each Dog Round. So Dog Rounds #1 and #2 have **6 dogs solo**, and #3 onward have **8 dogs solo**. Co-op is `players * 6` / `players * 8` (2 players: 12 / 16).
- **Max alive at once:** the spawn loop waits while `get_enemy_count() >= valid_players * 2`, polling every 2 s. Solo is **2 alive**; 4 players is 8. "Valid" means `is_player_valid` (downed players do not count).
- **Spawn cadence** (`waiting_for_next_dog_spawn`), applied after every spawn:

```
base = 3.0 (dog_round_count 1) | 2.5 (2) | 2.0 (3) | 1.5 (4 and up)
wait(base - count / max)       // count = dogs spawned so far, so the gap shrinks through the Round
```

- **Where:** each dog goes to a **dog spawn struct**. Per-Zone structs are named `<zone volume target>_dog` and gathered into `level.enemy_dog_locations` for active Zones ([T5] `_zombiemode_zone_manager.gsc` L224, L672-708). `dog_spawn_factory_logic` (set as `level.dog_spawn_func` on Der Riese, Kino and Shi No Numa) shuffles the list and takes the first struct that is **more than 400 and less than 1000 units** from the target player. If none qualify it returns the first in the shuffled list. It never repeats the previous struct (`level.old_dog_spawn`).
- **Target player** (`get_favorite_enemy`): the valid player with the fewest dogs assigned (`hunted_by`), so dogs spread across players.
- **Lightning spawn** (`dog_spawn_fx`): the dog spawns hidden with a magic bullet shield and the lightning FX plays at the struct. After **1.5 s** the bolt plays and the dog is teleported in facing the target. 0.1 s later it is shown, set to attackable, and runs. In WaW the teleport happens before the 1.5 s wait. The dog cannot be damaged or scored on while hidden. The 1.5 s runs in a separate thread, so it does not delay the next spawn.
- **No Barriers:** dogs are placed by `spawn_zombie` plus `ForceTeleport` to the struct. They do not use window spawners, never claim a Barrier, and need no board tear-down. Their `dog_clips` entities switch on for the duration (`dog_clip_monitor`), which closes some paths so dogs cannot take shortcuts.
- Hellhounds are not limited to a player's current Zone; the struct choice only needs the Zone to be active and to have dog structs.

## 3. Dog health, speed, attack

**Health** (`dog_health_increase`, [T5] and [T4] identical). Applied at the start of each Dog Round:

| Dog Round # (`dog_round_count`) | Health |
|---|---|
| 1 | **400** |
| 2 | **900** |
| 3 | **1300** |
| 4 and later | **1600** (the cap) |

- Dogs spawn with `dog_health * scr_dog_health_walk_multiplier` (4.0), which only applies while hidden. `dog_run_think` trims them to `level.dog_health` once they become visible. **Effective health is the table above.**
- The health value is **per Dog Round count, not per Round number**. The 4th Dog Round might land on Round 20 or Round 27; either way it is 1600.
- `level.dog_health` starts at 100 and keeps whatever the last Dog Round set. This matters for dogs spawned outside Dog Rounds (section 6, 8): they use the last value, normally 1600.
- Thundergun: `dog_thundergun_disintegrate` kills outright; `dog_thundergun_knockdown` deals 50% of max health ([T5]).

**Movement speed:** `self.zombie_move_speed = "sprint"` ([T5] `dog_init`). The actual units-per-second are in the run animation (`zombie_dog` animset), not in any GSC number, so a speed figure **cannot be taken from the scripts**. Use "faster than a sprinting zombie, roughly player run speed or above" as a design intent only (unverified). Dogs run as soon as they appear.

**Attack:**
- `self.meleeDamage = 40` ([T5] `dog_init`). The saved dvar is `dog_MeleeDamage = 100`, with a comment saying it is "rounded down to 40" after the melee multiplier. So a bite is **40 damage** (the player has 100 health).
- Attack range: `meleeAttackDist = 64` and `pathEnemyFightDist = 64` once spawned. During Dog Rounds `ai_meleeRange` is set to `100`, and `ai_meleeWidth/Height` to `0`. Der Riese raises it to **130** (`level.dog_melee_range = 130`, "to fix non-attacking dogs"); Kino's magic box room sets 120.
- Bite rate: set by the attack animation's note tracks (`dog_melee` in `zombie_dog_combat.gsc`), so **no number in the scripts**. Not verified.
- Each bite also plays a `dog_bite` shellshock for 0.35 s.

**Fire dogs and the death explosion** (`dog_fx_trail`, `dog_death`):
- `randomint(100) > dog_fire_trail_percent (50)` gives an ash trail, otherwise a fire trail. That is 49 percent ash and **51 percent fire**. If `!is_mature()`, always ash.
- A fire dog sets `a.nodeath = true`; on death it is deleted and an explosion FX plays (`dog_explode_fx`, 5 s lifetime). **No damage code is attached to the explosion: it is cosmetic.** An ash dog plays its normal death animation.

## 4. Points

- Kills use the same `"death"` event as a normal zombie: `player_add_points("death", damagemod, damagelocation, true)`. The fourth argument (`is_dog`) is **not used** in the score code, so dogs are scored like zombies. Core-loop values apply ([T5csv], from the core-loop doc): base **50**, head/helmet bonus +50 (**100** total), neck +20 (70), torso +10 (60), melee kill +80 (**130**), burn +10.
- A ballistic knife kill uses `"ballistic_knife_death"` (death points plus the melee bonus).
- Non-lethal hits give **10 points** (`damage` / `damage_light`) through the generic `zombie_damage` path; the call exits early while the dog is still hidden under its magic bullet shield (`is_magic_bullet_shield_enabled`), so there are no free points for shooting at a spawn flash.
- Headshots count as for zombies (the hit location is passed through). Dog hit locations are animation-rig dependent and **not verified** from script.
- Dog kills grant no special Power-up roll, no extra Points, and no Round bonus. 20 percent of the time the killer says a "hellhound" kill line (audio only).
- Insta-Kill, Double Points and Nuke apply normally. Nuke kills dogs but skips the flame and gib effects (`_zombiemode_powerups.gsc`).

## 5. End-of-Dog-Round reward

`dog_round_aftermath` ([T5]):

1. When the last dog dies (`get_enemy_count() == 0 && level.zombie_total == 0`), `level.last_dog_origin` is stored and `last_dog_down` is notified.
2. BO1: `specific_powerup_drop("full_ammo", last_dog_origin)` drops a **Max Ammo** at the last dog's death position. WaW does the same by forcing the next normal drop to `full_ammo` (`zombie_powerup_index`) and calling `powerup_drop`.
3. 2 s later the fog and music return (`dog_stop`). 6 s after that, `dog_intermission` ends, so the next Round starts about **8 s after the last dog dies**.

- **Dogs do not drop Power-ups during the Round.** The usual drop roll lives in the zombie death path (`zombie_death_event`), which dogs do not use; they have their own `dog_death`. Only the guaranteed Max Ammo comes from them.
- If the last dog dies mid-air or off the navmesh, the Power-up still spawns at that position (`+40` height). Its normal timeout applies (`powerup_timeout`).
- Round wait (`round_wait`): during a Dog Round the Round ends after `wait(7)` plus the `dog_intermission` flag, rather than the usual "no zombies left" loop.

## 6. Mixed Rounds (dogs in normal Rounds)

Only when `level.mixed_rounds_enabled == 1`. In the stock BO1 and WaW scripts that is **Der Riese only** (`zombie_cod5_factory.gsc` L25; `nazi_zombie_factory.gsc` L21). Shangri-La has its own copy of the loop; Kino and Shi No Numa do not enable it.

In `round_spawning` ([T5] `_zombiemode.gsc` ~L3188-3270, [T4] ~L1530-1600), before each normal zombie spawn:

| Round number | Chance per spawn tick |
|---|---|
| > 30 | 3 percent |
| > 25 | 2 percent |
| > 20 | 2 percent |
| > 15 | 1 percent |
| 15 or lower | never |

- The `mixed_spawns < N` caps (1, 2, 3 on the lower bands) look like per-Round limits, but **`mixed_spawns` is never incremented anywhere**, so the caps are inert in both games.
- When the roll hits: for every occupied Zone, every adjacent, active, unoccupied Zone that has dog structs spawns a dog (`special_dog_spawn(undefined, 1)`) and decrements `zombie_total`. So one hit can produce several dogs when the player is in a Zone with several valid neighbours.
- `special_dog_spawn` refuses to spawn if **9 or more** dogs are already alive in BO1. In WaW the limit is **2**.
- Mixed dogs use the last `level.dog_health` (normally 1600) and do not drop Max Ammo.

## 7. Round-start and visual specifics

- **Announce:** a dark sting plays (`dog_start` music state in BO1), then the Round-start dialogue and the announcer line (`zmb_vox_ann_dogstart`, a player line about 4.5 s in). Spawning begins after the 7 s wait.
- **Round counter:** unchanged. A Dog Round is a normal Round number, with no separate display and no skipped number. Normal-zombie health scaling is untouched; the Round after a Dog Round resumes the usual curve.
- **Fog:** `clientnotify("dog_start")` pulls the fog in on each map's client script. For Kino the half-distance goes from 2332.79 to 302.622 units, with a blue-grey colour (0.46, 0.56, 0.62). `dog_stop` restores it. Der Riese and Shi No Numa have their own values. This is a map-by-map visual setting, not a global number.
- **Barriers:** not involved (section 2). Window boards are untouched during a Dog Round.
- `dog_clips` blockers switch on during Dog Rounds; they stay up while any dog is alive.
- Music override: `level.music_round_override` is set for the Round (BO1).

## 8. Der Riese teleporter hook

In `player_teleporting` ([T5] `zombie_cod5_factory_teleporter.gsc` ~L397; [T4] `nazi_zombie_factory_teleporter.gsc` ~L380), after the teleport and a 2 s wait:

```
if (time_since_last_teleport < 60000      // under 60 s since the previous teleport
    && level.active_links == 3            // all three teleporter links active
    && level.round_number > 20)
    special_dog_spawn(undefined, 4)       // 4 dogs, "The Dogs attack!"
```

- `sam_nospawn` plays: the Samantha "no Power-up for you" line.
- The teleporter's guaranteed Power-up (`teleporter_powerup` struct) is dropped 2 s after arrival, regardless of the dog branch.
- Cost 1500, cooldown 5 s, delay 2 s in the same file (not dog specific).
- The 4 dogs spawn one at a time with the section 2 cadence, using `dog_round_count` (usually 4+, so 1.5 s base) and the last `dog_health`. They spawn only if fewer than 9 (BO1) / 2 (WaW) are already alive.
- Der Riese also sets `level.dog_melee_range = 130` and `level.no_dog_clip = true`, and builds a `dog_blocker_clip` near the revive spot.
- Other arrival outcomes (zombie nuke on teleport, Power-up) are not dog related; see the core-loop research.

## 9. Other BO1 special Zombie types

One line each, from the stock scripts. Round rules read from the `*_round_tracker` code; none of these were checked in depth.

- **Dogs** (above): first 5-7, then every 4-5 Rounds.
- **Crawlers** (generic `crawler_round_tracker` in `_zombiemode.gsc`): first on Round 4, then `next_dog_round + 2`, or 4-5 later without dogs. Gated by `level.crawlers_enabled`, which no stock map sets. So **no crawler Rounds in the stock BO1 maps**.
- **Nova crawlers (Ascension):** not found in the Ascension scripts of the dump (grep for `crawler` / `nova` returned only traversals and the black hole bomb). **Unverified.**
- **Quads (Kino):** `Theater_Quad_Round` after `begin_quad_introduction`; a map-triggered intro that avoids Dog Rounds. Rule not fully read.
- **Monkeys (Ascension)** (`_zombiemode_ai_monkey.gsc`): waits for power on and a purchased Perk; first at `round + randomintrange(1, 4)`; then `round + randomintrange(4, 6)`; skips (+1) if no player holds a Perk. Pack size is `players + encounters` (or `players * 2` early), health 25/50/75 percent of zombie health by encounter.
- **Pentagon Thief** (`_zombiemode_ai_thief.gsc`): first `round + randomintrange(1, 4)`, then `+ randomintrange(4, 6)`; the same tracker shape as dogs and monkeys.
- **George Romero / Director (Call of the Dead)** (`_zombiemode_ai_director.gsc`): map-driven spawn and a Max Ammo reward Round `round + randomintrange(1, 4)` then `+ randomintrange(4, 6)`. Not a standard Round replacement. (Corrects the brief: napalm and sonic are not on Call of the Dead.)
- **Napalm and Sonic ("shrieker") zombies** are on **Shangri-La**, not Call of the Dead. Napalm: `round + RandomIntRange(1, 3)` between spawns. Sonic: `round + RandomIntRange(1, 4)` between spawns.

---

## WaW vs BO1 (dogs)

| | WaW [T4] | BO1 [T5] |
|---|---|---|
| Health, count, cadence, schedule | identical | identical |
| Visible after teleport vs fx wait | teleport first, then 1.5 s FX | 1.5 s FX, then teleport |
| `special_dog_spawn` live-dog cap | 2 | 9 |
| End reward | forces next drop to Max Ammo via `powerup_drop` | `specific_powerup_drop("full_ammo")` |
| Dog explosion on death | fire dog flag `nodeath` | same, plus `is_mature()` gate |
| Melee dvars in Dog Rounds | range 100, width 25, height 20 | range 100 (130 Der Riese), width/height 0 |
| Bite damage | `dog_MeleeDamage 100` (rounds to 40) | `meleeDamage = 40` set explicitly |
| Enable | `dogs_enabled` in map file; tracker in `init()` | `dogs_enabled` plus `enable_dog_rounds()` |

---

## Confidence summary

| Number | Confidence |
|---|---|
| First Dog Round 5-7; gap +4/+5; no solo/co-op rule | High (identical in [T4] and [T5]) |
| Dog count `players * 6` (count < 3), `players * 8` | High |
| Max alive `valid_players * 2` | High |
| Spawn cadence 3/2.5/2/1.5 minus `count/max`; 7 s start delay; 1.5 s FX | High |
| Spawn distance 400-1000 units from target | High |
| Health 400/900/1300/1600 per Dog Round count | High |
| Bite 40 damage; melee range 100/120/130 | High for script values; the dvar rounding comment is a developer note |
| Fire dog 51 percent, explosion cosmetic | High |
| Points: same as zombie kills | High for logic; point values rely on the core-loop CSV research |
| Max Ammo at last dog position; ~8 s to next Round | High |
| Mixed dogs on Der Riese only, rates 1/2/2/3 percent, caps inert, live cap 9 (BO1) / 2 (WaW) | High |
| Der Riese teleport dogs: under 60 s, 3 links, Round > 20, 4 dogs | High |
| Kino has Dog Rounds in BO1 | High (`enable_dog_rounds()` in `zombie_theater.gsc`) |
| Dog run speed (units/s), bite rate | **Not in scripts. Unverified** |
| Headshot hit location on dog rigs | **Unverified** |
| Nova crawlers on Ascension | **Not found in the dump. Unverified** |
| Fog values | Per-map client scripts; only Kino/Der Riese read |
| Special-type one-liners (section 9) | Medium; read from tracker code only |
