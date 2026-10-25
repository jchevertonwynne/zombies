# Nova Crawler, Napalm, Shrieker, legless Crawler and Der Riese teleporter numbers (BO1)

Context for issue #30. Every number below comes from shipped GSC unless it is marked otherwise. Locations are given as file plus function name, because line numbers drift between dumps.

## Sources

| Tag | Source | What it is |
|---|---|---|
| **[T5]** | [plutoniummod/t5-scripts](https://github.com/plutoniummod/t5-scripts) @ `70689b8`, under `ZM/` | BO1 stock zombies GSC. Paths below are relative to `ZM/`. |
| **[T4]** | [CallOfDutyModding/call_of_duty_world_at_war_mod_tools](https://github.com/CallOfDutyModding/call_of_duty_world_at_war_mod_tools) @ `b777df1`, under `raw/maps/` | WaW mod-tools scripts. Only used for the Der Riese teleporter comparison. |
| **[T5csv]** | `research/core-loop-numbers.md` (from `BO1/zombiemode.csv`) | Point values. Not re-fetched here. |

Key T5 files:

- Nova Crawler (internal name **quad**, `animname = "quad_zombie"`): `Maps/Moon/maps/_zombiemode_ai_quad.gsc`, `Maps/Moon/maps/zombie_moon_ai_quad.gsc`, `Maps/Moon/maps/zombie_moon.gsc`. Near-identical copies in `Maps/Five/maps/_zombiemode_ai_quad.gsc` (the Pentagon map, "Five") and `Maps/Kino der Toten/maps/_zombiemode_ai_quad.gsc`.
- Napalm: `Maps/Shangri La/maps/_zombiemode_ai_napalm.gsc`. Shrieker (internal name **sonic**): `Maps/Shangri La/maps/_zombiemode_ai_sonic.gsc`. Spawning: `Maps/Shangri La/maps/zombie_temple_spawning.gsc`.
- Gibbing and legless Crawlers: `Common/maps/_zombiemode_spawner.gsc` (`zombie_gib_on_damage`, `zombie_should_gib`, `derive_damage_refs`, `make_crawler`, `zombie_damage`), `Common/maps/_zombiemode.gsc` (`crawler_round_tracker`, `zombie_speed_up`).
- Der Riese teleporter: `Maps/Der Riese/maps/zombie_cod5_factory_teleporter.gsc` (`player_teleporting`), `Common/maps/_zombiemode_powerups.gsc` (`special_drop_setup`).

**Name mapping.** The community names do not match the script names. Nova Crawler = `quad`. Shrieker = `sonic` (`zombie_sonic`, `actor_zombie_sonic`). Napalm = `napalm`. The dump may differ from the shipped game if it was patched after the dump was taken; that cannot be checked from the scripts.

---

## 1. Nova Crawlers (quads)

### 1.1 Where they exist (important correction)

| Map | Evidence | Quads? |
|---|---|---|
| **Moon** | `zombie_moon.gsc`: `level.quad_explode = true;`, `maps\_zombiemode_ai_quad::init` in `custom_ai_type`, `maps\zombie_moon_ai_quad::init()` | **Yes** |
| Kino der Toten | `zombie_theater.gsc`: `quad_explode = true`, quad wave code | Yes (introduced here) |
| Five (Pentagon) | `zombie_pentagon.gsc`: `quad_explode = true`, quad init | Yes |
| **Ascension** | `zombie_cosmodrome.gsc` only sets `level.quad_move_speed = 35;`. Its `custom_ai_type` holds only `_zombiemode_ai_monkey::init`. There is no `_zombiemode_ai_quad.gsc` in `Maps/Ascension/`, no gas FX, and no `quad_zombie_spawner` reference | **No quad logic in the dump** |

So the prior subagent's "no Nova crawlers in the dump" is explained: **in this dump the gas crawler exists on Moon (plus Kino and Five) and Ascension has none.** The `quad_move_speed = 35` line on Ascension is a copy-paste leftover; the variable is assigned on all four maps and never read anywhere in `ZM/` (grep over `.gsc` and `.csc`). Ascension's only special AI in the scripts is the Space Monkey. I could not verify the community claim that Ascension has Nova Crawlers; if the shipped game has them, they are not in this script dump. Treat "Ascension Nova Crawler" as **unverified**.

### 1.2 Spawn rate, groups, entry

- **No quad-specific Round schedule, mix rate or group size exists in the Moon scripts.** Quads are ordinary zone spawners: `init()` does `GetEntArray( "quad_zombie_spawner", "script_noteworthy" )` and attaches `quad_prespawn`. Moon's normal `round_spawning` (`_zombiemode.gsc`) picks `level.enemy_spawns[ RandomInt( size ) ]` uniformly from the active-zone spawner list built by `create_spawner_list` (`_zombiemode_zone_manager.gsc`). So the quad share of spawns is `quad spawners / all spawners in active zones`. **That ratio lives in the BSP (`zombie_moon.d3dbsp`, binary) and cannot be read from the scripts.** Moon has no `ignore_spawner_func`, so quad spawners are never filtered.
- No Round gating on Moon: no `level.round_number` check anywhere in `_zombiemode_ai_quad.gsc` or `zombie_moon_ai_quad.gsc`. They can spawn from Round 1 wherever a quad spawner is in an active zone.
- Moon's special Round code is separate: `moon_round_think_func` (No Man's Land / teleporter handling) and the Astro (`_zombiemode_ai_astro.gsc`), neither affects quads.
- **Entry:** Moon has a quad-only traversal, `Maps/Moon/animscripts/traverse/zombie_quad_vent_jump_down.gsc` (anim `ai_zombie_quad_jump_down_vent`, then notifies `quad_end_traverse_anim`, which starts the trail FX). That supports "enter through a vent and drop down". Which spawners sit at vents is a BSP detail.
- **Kino only (for comparison):** `theater_ignore_spawner` excludes quad spawners until `curtains_done`; `time_for_quad_wave` can start a `special_quad_round` where only quad spawners are used for the first half of a Round, and skips a Dog Round. Moon has none of this.

### 1.3 Stats

| Stat | Value | Source |
|---|---|---|
| Health | `int( maxhealth * 0.75 )` where `maxhealth` is the normal zombie health for the Round (`150` at Round 1, `+100` per Round to Round 9, then `+10%` per Round from Round 10: `zombie_health_start/increase/increase_multiplier`). Aitype base `self.health = 150` is overwritten | `quad_prespawn`; `zombie_moon_quad.gsc` aitype; `_zombiemode.gsc` vars, `ai_calculate_health` |
| Melee damage | `45` per hit (normal zombie: `60`) | `quad_prespawn` (`self.meleeDamage = 45`) |
| Speed | Anim-driven. Uses the crawl run cycles (`ai_zombie_quad_crawl`, `_run`, `_sprint`); extra sidestep and "phase" anims. **No unit-per-second value in script** (`quad_move_speed` is unused) | `init_quad_zombie_anims`, `moon_quad_prespawn` |
| Collision | `setPhysParams( 15, 0, 24 )` (short, so players can jump over) | `quad_prespawn` |
| Gibbing | `self.no_gib = true` on Moon (cannot be gibbed into a legless Crawler) | `moon_quad_prespawn` |
| Moon extras | Can sidestep, forward-step and **phase** (`hide()` for the anim, `show()` at the end, with FX and sounds); low-gravity "super sprint" anim | `zombie_moon_ai_quad.gsc` |
| Freeze gun | `freezegun_damage = 0` | `quad_prespawn` |
| Leap attack | Fully written (`quad_zombie_think`, `TryLeap`, leap 96-144 units, 3 s cooldown) but **the thread call is commented out** (`//self thread ...quad_zombie_think()`), so it is dormant | `quad_prespawn`, `check_wait` |
| Thundergun | Disintegrates (`DoDamage( health + 666 )`) on a full hit; knockdown does `0.5 * maxhealth` | `quad_thundergun_*` |
| Points | No quad-specific scoring: same kill and damage points as a normal zombie (see core-loop research [T5csv]) | `zombie_death_points` path unchanged |

### 1.4 Gas cloud (the "Nova" part)

Mechanic comment in the script: "TEMP version of the Quad Gas Explosion Mechanic". Variables set in `quad_prespawn`:

```
death_explo_radius_zomb = 96     // blast radius vs zombies
death_explo_radius_plr  = 96     // blast radius vs players
death_explo_damage_zomb = 1.05   // x maxhealth, but the loop that used it is commented out
death_gas_radius        = 125    // gas trigger radius
death_gas_time          = 7      // seconds
```

Behaviour, in order (`quad_post_death`, `quad_gas_explo_death`, `quad_death_explo`, `quad_gas_area_of_effect`):

1. **Gate:** only if `level.quad_explode == true` (set on Moon). `quad_killed_override` sets `can_explode = true` **only when the killing blow is `MOD_PISTOL_BULLET` or `MOD_RIFLE_BULLET`**. Melee, explosives, fire, Tesla and the Thundergun do not trigger the gas. The Moon hook `_override_quad_explosion` also forces `can_explode = false` when the quad is in **low gravity** (`in_low_gravity`), so no gas outside the pressurised areas.
2. **Blast:** sound `zmb_quad_explo`, dog-gib FX. Players within **96 units** get `ShellShock( "explosion", 2.5 )` (2.5 s), unless immune. The player-blast damage is **not** applied (no `DoDamage` to players). `RadiusDamage( origin, 96, level.zombie_health, level.zombie_health, self, "MOD_EXPLOSIVE" )` is issued on the quad's origin; the per-zombie loop with `explo_damage_zomb` is commented out. In effect: **other zombies within 96 units take a full Round's zombie-health of explosive damage (a kill for ordinary zombies)**. Treat the zombie-kill effect as medium confidence: it depends on how `RadiusDamage` treats the dying quad as the inflictor.
3. **Gas:** the quad entity is deleted; a `trigger_radius` of **125 units**, height **100**, plays `fx_zombie_quad_gas_nova6` for **7 seconds** (loop `while gas_time <= 7` with `wait(1)`, so 8 ticks, about 8 s). Each second, players touching it and not immune get `setblur( 4, 0.1 )`; otherwise `setblur( 0, 0.5 )`. **No health damage and no effect on zombies.**
4. **Immunity:** `moon_quad_gas_immune` returns true if the player has the **gas mask equipment active** (`gasmask_active()`). Immune players skip both the shell shock and the blur. The Five copy has no immunity function.
5. Screen effect therefore = shell shock 2.5 s plus blur strength 4 while standing in the cloud. No point loss and no tangible damage.

### 1.5 Killing up close

The only "punishment" in the scripts is the gas above. It requires a **bullet kill**; there is no distance check. Being within 96 units gives a 2.5 s shell shock, being inside 125 units gives blur for the cloud's lifetime. Melee-killing a quad avoids all of it (and the gas mask removes it). There is no points penalty.

---

## 2. Napalm Zombie (Shangri-La)

### 2.1 Cadence

From `napalm_zombie_init`:

```
napalmZombieMinRoundWait = 1
napalmZombieMaxRoundWait = 2
napalmZombieRoundRequirement = 5
nextNapalmSpawnRound = 5 + RandomIntRange( 0, 3 )      // 5, 6 or 7
```

- After a Napalm dies (`napalm_zombie_count_watch`): `nextNapalmSpawnRound = round_number + RandomIntRange( 1, 3 )` = **+1 or +2** Rounds. If it died to the Shrink Ray: **+1** exactly.
- `RandomIntRange(a, b)` excludes `b`. So first Napalm Round is 5, 6 or 7 with equal odds, then every 1 or 2 Rounds.
- **Once the Round qualifies** (`_can_spawn_napalm`): it spawns when `zombie_total < RandomIntRange( 25%, 75% of the Round's total )`, i.e. between a quarter and three quarters of the way through the Round. `_try_spawn_napalm` is tried first on every spawn tick, ahead of the sonic and normal zombies.
- **Limits:** one alive at a time (`napalmZombieCount`), and at most one special zombie (Napalm or Shrieker) per Round (`special_zombie_spawned_this_round`). It can carry over into the next Round (`ignore_enemy_count = true`). Napalm therefore wins ties against the Shrieker.
- **Spawn point:** a `special_zombie_spawn` struct, first from occupied Zones, otherwise from active Zones. Spawns underground, rises (45 units down), invulnerable (magic bullet shield) during the rise.

### 2.2 Stats

| Stat | Value |
|---|---|
| Health | `maxhealth * players * 4` (`napalmHealthMultiplier = 4`, times player count; a 4-player game is 16x a normal zombie) |
| Speed | `set_zombie_run_cycle( "walk" )`: walk cycle (`ai_zombie_napalm_run_01..03`); `_napalm_nuke_damage` does not switch to sprint |
| Gibbing | `no_gib = true` |
| Melee/explosion damage | `napalm_custom_damage` returns `meleeDamage`, or `overrideDeathDamage` when set |
| Damage taken | Insta-Kill only **doubles** damage (the instakill func is undefined). **Wet** (4 s after touching water or sliding): damage x5. Standing in water: x2. Spikemore: 0. |

### 2.3 Explosion and fire

All from `_zombie_ExplodeNearPlayers`, `napalm_zombie_death`, `_napalm_damage_players`:

| Number | Value |
|---|---|
| Self-explode trigger | A valid player within **90 units (2D)**, Z difference at most 50, not rising, and at least 2 s after spawn. Plays a windup (`ai_zombie_napalm_attack_01` at 2x speed), then kills itself (`killed_self`) |
| Heat warning radius | **400** (`napalmExplodeDamageRadius`): players inside get `SetBurn( 10 )` and a burning loop, cleared on leaving or on its death |
| Zombie kill radius | **250** (`napalmZombieDamageRadius`): every other zombie and dog with line of sight dies (`health + 666`), random gib ref, flame death FX unless the Napalm is in water |
| Player damage, distance `<90` | **250** (kills a Juggernog player) |
| Player damage, `90-150` | **100** (kills a non-Jug player at 100 health) |
| Player damage, `150-400` | Linear from 100 down to 50: `50 + 50 * (400 - dist) / 250`, shellshock 1.5-3 s |
| Modifiers | x0.25 if the Napalm was shrunk, x0.25 more if it was wet. Wet also shrinks the damage radius to 250 |
| Shell shock | `explosion`, up to 3 s (scaled as above) |
| Line of sight | Eye, mid-body or feet trace must pass or the player is skipped |
| Fire pool on death | `trigger_radius` **80 units**, height 70, **20 s**, 40 damage per hit with a 500 ms debounce (about 80 damage per second). Not spawned if the Napalm dies in water. Non-Napalm AI that touch fire are killed (`kill_with_fire`); max 6 burning zombies at once |
| Fire pool on spawn | Radius 80, **6 s**, **10** damage per 500 ms ("don't want spawn fire to kill the player, it feels cheap") |

### 2.4 Points and rewards

- Per-hit damage points: **none** (`no_damage_points = true`, and `_napalm_damage_callback` returns true, which stops normal scoring).
- Kill: the killer gets the normal `zombie_death_animscript` kill points (50 base plus hit-location bonus [T5csv]).
- **Bonus:** if a player kills it (not `killed_self`, not shrunk), **every valid player gets `player_add_points( "thundergun_fling", 300 )` = 300 points**, passed through the double-points multiplier. If it explodes on its own near a player, there is **no reward**.
- Player achievement hook: `zomb_disposal_achieved` when nobody was hurt by the blast.

---

## 3. Shrieker (Sonic Zombie, Shangri-La)

### 3.1 Cadence

From `sonic_zombie_init`:

```
sonicZombieMinRoundWait = 1
sonicZombieMaxRoundWait = 3
sonicZombieRoundRequirement = 4
nextSonicSpawnRound = 4 + RandomIntRange( 0, 4 )       // 4, 5, 6 or 7
```

- After a death: `round_number + RandomIntRange( 1, 4 )` = **+1, +2 or +3** Rounds. Shrink-Ray kill: +1.
- Mid-Round window identical to Napalm (`zombie_total < RandomIntRange( 25%, 75% )`). One alive at a time, one special per Round (Napalm is checked first), carries over into the next Round.
- So the first-pass "1-2 and 1-3" cadence from #29 is right: Napalm **1-2**, Shrieker **1-3**; first appearances Round **5-7** (Napalm) and **4-7** (Shrieker).

### 3.2 Stats

| Stat | Value |
|---|---|
| Health | `int( maxhealth * 2.5 )` (`sonicHealthMultiplier`). The per-player multiplier is commented out, so it does **not** scale with player count |
| Speed | `set_zombie_run_cycle( "sprint" )`: sprints from spawn. Has side-step anims (`_zombie_InitSideStep`) |
| Spawn | Rises from the ground at 1.5x speed with a scream and FX, invulnerable until the rise finishes |
| Gibbing | `gibbed = true` (blocks the random gib path; the death scream still pops heads of other zombies) |

### 3.3 Scream

| Number | Value |
|---|---|
| Starts a scream when | a valid player is inside the **blur area**: 2D distance <= **300** (`sonicScreamDamageRadius`), Z difference <= 70, and in front of the Shrieker (`dot( dirToPlayer, forward ) >= 0.4`, about a 66 degree half-cone). The code comments say "attack radius 240 (20 ft)" but `_zombie_getNearByPlayers` using it is not what gates the scream; the gate is the blur area |
| Debounce | Global (level) **3-9 s** between any screams; per Shrieker **6-10 s** (random each time) |
| Player effect | `SetBlur( 5, 0.2 )` for **4.0 s**, `Earthquake( 0.2, 3, ..., radius 300 )`, heavy rumble. Shrunk Shrieker: blur 2 for 1.0 s, light rumble |
| Player damage | **None.** `sonicPlayerDamage = 10` is declared and never used |
| Scream duration | FX/sound for 2.0 s after the `fire` notetrack; the animation stops early if no player remains in the blur area, if the Shrieker is shrunk or loses its head (`head_gibbed`) |
| Drawing zombies | **Nothing found.** The live scream only affects players; no pathing, attraction or POI code appears in `_zombiemode_ai_sonic.gsc` |
| Death scream | On death, every axis AI within **480** units with line of sight pops its head and dies after a random 0-1 s (plus a taunt animation if it has legs and about 0.75-1x the taunt length). Those zombies are flagged `no_powerups`. The older fling/knockdown "death explode" (ranges 240/360/480) is **commented out / unreferenced** |

### 3.4 Points

- `_sonic_damage_callback` returns true for the Shrieker, replacing normal damage scoring. Every `int( players * 2.5 )`-th hit awards `10` points (`thundergun_fling`): solo = every 2nd hit, 2 players = every 5th, 4 players = every 10th.
- Insta-Kill still works through the callback (`check_for_instakill`).
- The kill itself uses the normal kill points, and each zombie killed by the death scream is a normal kill for the attacker (50 base plus bonuses, no head-shot bonus because it is a script kill).
- There is no flat completion bonus like the Napalm's 300.

---

## 4. Legless Crawlers from explosive damage (any map)

### 4.1 Rules (`zombie_gib_on_damage`, `zombie_should_gib`)

Every damage event runs through `zombie_should_gib`. It **rejects** a hit when:

- the build is not mature or is the German build;
- `self.no_gib == 1` (Moon quads, Napalm);
- the damage is Freeze Gun damage;
- the damage type is `MOD_UNKNOWN`, `MOD_CRUSH`, `MOD_TELEFRAG`, `MOD_FALLING`, `MOD_SUICIDE`, `MOD_TRIGGER_HURT`, `MOD_BURNED` or `MOD_MELEE` (melee never gibs);
- a bullet hit where the attacker is not a player, holds `none` or the M1911, or the victim holds a gas weapon;
- **`damage / (damage + health_after) * 100 < 10`**: the hit must be at least **10 percent of the zombie's health before the hit**. Overkill (`health <= 0`) clamps the denominator to 1 and passes.

Headshots take a separate path (`head_should_gib`), where grenade-type damage (`MOD_GRENADE`, `MOD_GRENADE_SPLASH`) is handled at the grenade branch around line 2736.

If the hit passes and the head did not gib, `self.damageLocation` picks the gib refs:

| Hit location | Refs (random pick from the list) |
|---|---|
| `torso_upper/lower` | `guts`, `right_arm` |
| right arm / hand | `right_arm` |
| left arm / hand | `left_arm` |
| right leg upper/lower/foot | `right_leg` x3, `no_legs`, **only added if `health <= 0`** |
| left leg upper/lower/foot | `left_leg` x3, `no_legs`, **only if `health <= 0`** |
| anything else with a known location | 6 refs, one each: `guts`, `right_arm`, `left_arm`, `right_leg`, `left_leg`, `no_legs` |
| location `"none"` and type `MOD_GRENADE`, `MOD_GRENADE_SPLASH`, `MOD_PROJECTILE`, `MOD_PROJECTILE_SPLASH` | `derive_damage_refs( point )` |
| location `"none"` and any other type (for example `MOD_EXPLOSIVE`) | no refs, no gib |

`derive_damage_refs` finds the **nearest of 15 joints** to the impact point and maps it: three spine joints -> {`guts`, `right_arm`}; three left-arm joints -> `left_arm`; three right-arm joints -> `right_arm`; left hip/knee/ankle -> {`left_leg`, `no_legs`}; right hip/knee/ankle -> {`right_leg`, `no_legs`}. So an explosion that lands nearest a leg joint (usually one detonating at the feet) has a **50/50** split between "one leg" and "both legs", and anything else cannot make a Crawler.

**A zombie becomes a legless Crawler only if the chosen ref is `no_legs`, `right_leg` or `left_leg` and `self.health > 0` after the hit.** Consequences:

1. **Bullet hits to legs never create Crawlers** in this code: the leg cases only add refs once `health <= 0`, but the Crawler branch needs `health > 0`. Leg shots on a killing blow only gib the limb. Crawlers come from the "other location" and derived-location paths.
2. **Explosives** (grenade, grenade splash, launcher, projectile splash with location `none`) use the nearest-joint rule above, so the chance is the probability that the nearest joint is a leg joint, not a flat percentage. If the engine passes a real hit location (not `none`), the 6-way uniform pick applies: **3 of 6 (50 percent)** are leg refs.
3. The kill must not be the same hit, and the hit must be at least 10 percent of current health.

Extra damage for explosive types (`zombie_damage`): `MOD_GRENADE/_SPLASH` additionally deals `round_number + RandomIntRange( 100, 200 )`; `MOD_PROJECTILE`, `MOD_EXPLOSIVE`, `MOD_PROJECTILE_SPLASH` deal `round_number * RandomIntRange( 0, 100 )`. These bonuses are part of why a splash hit usually satisfies the 10 percent rule at low Rounds.

### 4.2 Crawler properties (`zombie_gib_on_damage`, `make_crawler`)

| Property | Value |
|---|---|
| State | `has_legs = false`, `AllowedStances( "crouch" )`, `needs_run_update` |
| Collision | `setPhysParams( 15, 0, 24 )`: short, jumpable |
| Move anim | `no_legs`: hands-walk (`crawl_hand_1` or `_2`, 50/50). One-leg: random `crawl1..5` (1 in 5 each). Death anims `crawl_death_v1/v2` |
| Speed | Animation-driven; **no numeric speed in script**. Crawl-sprint anims exist (`crawl_sprint1..3`) but nothing in `Common/` assigns them |
| Reach | `meleeAttackDist = 64` and `pathEnemyFightDist = 64`, the same as a standing zombie. Melee uses the crawl melee anims (`level._zombie_melee_crawl`) |
| Health | Unchanged. (`health = health * 0.1` is computed in the gib code and never used) |
| Boards | No board attack through the window and no taunt (`should_attack_player_thru_boards` and `do_a_taunt` return false when `!has_legs`) |
| Knockdown | A different knockdown animation set (`no_legs` list); pain code skips legless zombies |
| Vox | A 10 percent chance for the shooting player to say `crawl_spawn` when it is a `no_legs` gib |

### 4.3 Round end

- `zombie_speed_up` only upgrades the **last** zombie to a sprint when `has_legs == true`. **A lone legless Crawler never gets the end-of-Round sprint.** Nothing else in the Round loop treats Crawlers differently (no despawn, no special wait); `round_spawn_failsafe` is unchanged.
- **Dedicated Crawler Rounds exist but are disabled.** `level.crawlers_enabled` is commented out in `_zombiemode.gsc` and no stock map sets it. If enabled: first Crawler Round is 4; afterwards `next_dog_round + randomintrange( 2, 3 )` (always +2) when a dog schedule exists, else `randomintrange( 4, 6 )`. The code that would turn spawns into Crawlers (`make_crawler()` call in `zombie_spawn_init`) is commented out too. `make_crawler()` itself would make a leg-gibbed zombie with the ref weights above (3 x one leg : 1 x `no_legs`).

---

## 5. Der Riese teleporter arrival outcomes

### 5.1 Logic

After a teleport (`player_teleporting`): `wait( 2.0 )`, then if the `teleporter_powerup` struct exists, `special_powerup_drop( origin )` runs. That function validates the drop point and calls `special_drop_setup()`.

`special_drop_setup` in BO1 (and the WaW copy has the same shape):

```
if round_number <= 10:           powerup = get_valid_powerup()           // always a real powerup
else:
    powerup = special_drop_array[ RandomInt( size ) ]
    if round_number > 15 and RandomInt(100) < (round_number - 15) * 5:
        powerup = "nothing"
```

**What is in `special_drop_array`:** `init_powerups` adds `"dog"` first; `add_zombie_powerup` appends every powerup the map has *included*. Der Riese's `include_powerups()` includes **nuke, insta_kill, double_points, full_ammo, carpenter**. So the array has **6 entries: dog, nuke, insta_kill, double_points, full_ammo, carpenter**. The commented line `add_zombie_special_drop( "nothing" )` is dead. The same 6 entries apply in WaW (`nazi_zombie_factory.gsc` includes the same five).

Switch outcomes:

- `nuke`, `insta_kill`, `double_points`, `carpenter`: drop that powerup (the teleporter Carpenter has no "needs 5 broken windows" check, unlike `get_valid_powerup`).
- `full_ammo`: if `round_number > 10` and `RandomInt(100) < (round_number - 10) * 5`, replace it with a uniformly random entry of `zombie_powerup_array` (so it can re-roll to Max Ammo again).
- `dog`: if `round_number >= 15` -> **no powerup**; otherwise a normal `get_valid_powerup()`.
- `nothing`: lightning FX (`lightning_dog_spawn`) at the pad, `pre_spawn` sound, **1.5 s** wait, bolt sound, `Earthquake( 0.5, 0.75, ..., 1000 )`, rumble, `spawn` sound, 1 s wait, the `sam_nospawn` line, and the powerup entity is deleted. This is the "lightning flash with nothing" outcome.

### 5.2 BO1 vs WaW: the "dog" outcome

| | BO1 [T5] | WaW [T4] |
|---|---|---|
| `dog` at Round >= 15 | `maps\_zombiemode_dogs::special_dog_spawn( dog_spawners, 1 )` is **commented out** ("Z2 comment out for now so we don't need to include _zombiemode_dogs"). It plays `sam_nospawn` only: **0 dogs, no powerup, no lightning flash** | `special_dog_spawn( dog_spawners, 1 )` runs: **1 dog** |
| Last-stand clause | none | `round_number <= 10 or player_num_in_laststand()` also forces a real powerup |

### 5.3 Odds table (BO1, Der Riese)

`R` = Round number when the powerup is requested (2 s after arrival). `n = min(1, (R-15)*0.05)` is the "nothing" override for R > 15. `dog` has weight 1/6 before the override.

| Round | Real powerup | Dog outcome | Lightning flash, nothing |
|---|---|---|---|
| 1-10 | 100% | 0 | 0 |
| 11-14 | 100% (the dog pick falls back to a powerup) | 0 | 0 |
| 15 | 83.3% (5/6, no override at exactly 15) | **16.7%** (0 dogs in BO1, 1 in WaW) | 0 |
| 20 | 62.5% (5/6 x 75%) | 12.5% (1/6 x 75%) | 25% |
| 25 | 41.7% | 8.3% | 50% |
| 30 | 20.8% | 4.2% | 75% |
| 35+ | 0% | 0% | **100%** |

General form for R >= 15: nothing = `n`, dog outcome = `(1 - n) / 6`, real powerup = `(1 - n) * 5/6` (with the Max Ammo re-roll nudging within the powerup share).

### 5.4 The separate teleporter dog attack

Also in `player_teleporting`, after the powerup step: if `time_since_last_teleport < 60000` ms **and** `level.active_links == 3` **and** `round_number > 20`, then `special_dog_spawn( undefined, 4 )` spawns **4 Hellhounds** (via the Zone dog structs, subject to the 9-dog live cap in `special_dog_spawn`) and plays `sam_nospawn`. This is independent of the powerup roll and exists in both BO1 and WaW (`maps\_zombiemode_dogs`). It is the only place the teleporter actually spawns dogs in BO1.

So the answer to "Power-up vs dogs vs nothing after Round 10": Power-up always up to Round 14; from Round 15 a growing chance of nothing (+5 percentage points per Round from Round 16, certain by Round 35), and a flat 1/6 slice (before the nothing override) of a silent "dog" result that spawns **no dog in BO1** (1 dog in WaW). The 4 dogs come only from re-teleporting within 60 s with all three links on, after Round 20.

---

## WaW vs BO1 summary

- Teleporter `dog` outcome spawns 1 dog in WaW, 0 in the BO1 dump (5.2).
- Teleporter powerup cadence rules otherwise identical.
- The Nova/Napalm/Shrieker types do not exist in WaW.
- Gib logic: BO1 requires `is_mature()`/German-build checks, a 10 percent damage rule and weapon filters shown above; the WaW copy was not diffed.

## Confidence summary

| Number | Confidence |
|---|---|
| Quad class has gas on Moon, Kino, Five; none on Ascension in the dump | High for the dump; **Ascension Nova Crawler in the shipped game: unverified** |
| Quad health 0.75x, melee 45, no_gib on Moon, collision `15, 0, 24` | High |
| Gas: radius 125, 7 s (8 ticks), blur 4, shell shock 2.5 s within 96; bullet kills only; gas mask immune; no damage to players | High |
| Gas blast kills zombies within 96 via `RadiusDamage` | Medium (the dedicated loop is commented out) |
| Quad mix rate, group size, Round gating | **Not in scripts. BSP spawner counts: unverified** |
| Quad speed (units/s) | **Not in scripts. Unverified** |
| Vent entry for Moon quads | Medium (traversal anim exists; spawner placement is BSP) |
| Napalm: first Round 5-7, then +1/+2, mid-Round spawn, 1 alive, 1 special per Round | High |
| Napalm health x players x 4; wet x5 / water x2 | High |
| Napalm explosion radii 90/150/400, damage 250/100/100-50, kill radius 250, fire 80 units for 20 s at 40 per 0.5 s | High |
| Napalm reward 300 to every valid player on a shot kill | High |
| Shrieker: first Round 4-7, then +1/+2/+3; health x2.5 flat | High |
| Scream: 300 radius cone, blur 5 for 4 s, 3-9 s global and 6-10 s own debounce, no damage | High |
| Scream "draws zombies" | **No code found. Unverified / probably not a feature** |
| Death scream kills zombies within 480 with line of sight | High |
| Crawler gib: 10 percent damage rule, ref lists, leg shots need a kill (bullets never make Crawlers) | High (read directly) |
| Explosive Crawler odds as a flat percent | **Not a flat percent in script; nearest-joint (about 50/50 leg vs no_legs) or 50 percent uniform** |
| Crawler speed / reach in units | Reach 64 (same as walkers); speed **not in scripts** |
| End-of-Round: legless last zombie never sprints | High |
| Crawler Rounds (disabled): first at 4, then +2 after the dog schedule | High |
| Der Riese special-drop array has 6 entries; Round <= 14 always powerup; nothing = (R-15) x 5 percent from Round 16 | High |
| BO1 teleporter "dog" outcome spawns no dog; WaW spawns 1 | High (commented-out call vs live call) |
| Teleporter 4 dogs: under 60 s, 3 links, Round > 20 | High |
| Player-facing names (Nova Crawler, Shrieker) vs script names (quad, sonic) | High for the mapping by FX/class name (`fx_zombie_quad_gas_nova6`, `actor_zombie_sonic`) |
