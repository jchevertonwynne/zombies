# Original core-loop numbers (WaW / BO1)

Answers ticket `.scratch/zombies-v2/issues/01-core-loop-numbers.md`. BO1 is the baseline; WaW differences are called out.

## Sources

Most numbers come from the shipped game scripts. Tables are the `mp/zombiemode.csv` files the scripts read.

| Tag | Source | What it is |
|---|---|---|
| **[T5]** | [plutoniummod/t5-scripts](https://github.com/plutoniummod/t5-scripts), `ZM/Common/maps/_zombiemode*.gsc`, `_gameskill.gsc`, `ZM/Maps/Kino der Toten/maps/zombie_theater.gsc` | BO1 stock zombies GSC (Plutonium T5 dump) |
| **[T5csv]** | [akaTrxgic/CoD-Tables `BO1/zombiemode.csv`](https://github.com/akaTrxgic/CoD-Tables) (same byte size as [JTAG7371/T5-RawFile-Dump](https://github.com/JTAG7371/T5-RawFile-Dump) `mp/zombiemode.csv`) | BO1 tuning table. **It overrides the GSC defaults.** |
| **[T4]** | [CallOfDutyModding/call_of_duty_world_at_war_mod_tools](https://github.com/CallOfDutyModding/call_of_duty_world_at_war_mod_tools), `raw/maps/_zombiemode*.gsc` | Treyarch's WaW mod-tools scripts (the Der Riese-era common code) |
| **[T4n]** | [plutoniummod/t4-scripts](https://github.com/plutoniummod/t4-scripts), `SP/ZM Maps/Nacht der Untoten/maps/` | WaW Nacht der Untoten's own script copy |
| **[T4csv]** | [akaTrxgic/CoD-Tables `WAW/zombiemode.csv`](https://github.com/akaTrxgic/CoD-Tables) | WaW tuning table |

How BO1 reads the table: `set_zombie_var(name, default, is_float, column)` looks up the CSV first and falls back to the default only when the cell is empty ([T5] `_zombiemode_utility.gsc` `set_zombie_var`). Zombies always uses `difficulty = 1`, so `column = 2` is the **MEDIUM** column ([T5] `_zombiemode.gsc` ~L685, L1413). This matters because some GSC defaults never apply in game. For example, the starting-points default in GSC is 3000, but the CSV makes it 500.

---

## 1. Zombie Health per Round

BO1 ([T5] `ai_calculate_health`; values from [T5csv] MEDIUM):

```
health = 150                         // zombie_health_start
for i in 2..=round:
    if i >= 10: health += int(health * 0.1)   // zombie_health_increase_multiplier
    else:       health  = health + 100        // zombie_health_increase
```

- Round 1 has 150 health. Each round up to round 9 adds 100, so round 9 has 950.
- From round 10, health grows by about 10% per round, truncated to an integer each step.
- WaW uses the same curve ([T4] `zombie_health_start 150`, `zombie_health_increase 100`, `zombie_health_increase_percent 10/100`). The only difference is that WaW applies the increase once per round instead of recomputing from round 1, which gives the same values.

| Round | 1 | 2 | 3 | 5 | 9 | 10 | 11 | 12 | 15 | 20 | 25 | 30 | 40 | 50 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| HP | 150 | 250 | 350 | 550 | 950 | 1045 | 1149 | 1263 | 1679 | 2701 | 4348 | 7000 | 18151 | 47073 |

## 2. Horde size per Round (solo)

BO1 ([T5] `round_spawning` and `default_max_zombie_func`):

```
max  = 24                              // zombie_max_ai
mult = max(1, round / 5)               // float division
if round >= 10: mult *= round * 0.15
solo:  max += int(0.5 * 6 * mult)      // zombie_ai_per_player = 6
coop:  max += int((players-1) * 6 * mult)
early-round scale: r1 ×0.25, r2 ×0.3, r3 ×0.5, r4 ×0.7, r5 ×0.9 (int)
```

WaW Verrückt/Shi No Numa/Der Riese ([T4]) use the same base formula. The early-round scale differs: r1 ×0.2, r2 ×0.4, r3 ×0.6, r4 ×0.8, and none on r5. On Verrückt, rounds 1–2 are a fixed 6 zombies in solo, or `players*3 + round` in co-op.

WaW Nacht ([T4n]) has **no solo bonus** (`max += (players-1)*6*mult`). In solo it is therefore 24 zombies per round from round 5 onward, forever.

| Round | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 12 | 15 | 20 | 25 | 30 | 50 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| BO1 solo | 6 | 8 | 13 | 18 | 24 | 27 | 28 | 28 | 29 | 33 | 36 | 44 | 60 | 80 | 105 | 249 |
| WaW (Der Riese) solo | 5 | 10 | 16 | 21 | 27 | 27 | 28 | 28 | 29 | 33 | 36 | 44 | 60 | 80 | 105 | 249 |
| WaW Nacht solo | 4 | 9 | 14 | 19 | 24 | 24 | … | | | 24 | | | | | | 24 |

## 3. Alive Cap

- **24** in both games: `level.zombie_ai_limit = 24` with `SetAILimit` ([T5] `_zombiemode.gsc` L83–86; [T4] `SetAILimit( 24 )`).
- The spawn loop waits while `get_enemy_count() >= zombie_ai_limit` ([T5] `round_spawning`).
- Spawn points are picked at random from the active zone spawners, with one re-roll if the same point comes up twice in a row.

## 4. Spawn delay per Round

- BO1 ([T5] `round_think`, [T5csv] `zombie_spawn_delay 2.0`): round 1 spawns one zombie every **2.0 s**, plus one network frame.
- At the end of each round: `if delay > 0.08 { delay *= 0.95 } else if delay < 0.08 { delay = 0.08 }`. The floor is about 0.08 s and is reached around round 64.
- The formula is `delay(round) = 2.0 * 0.95^(round-1)`. Sample values: r5 1.63, r10 1.26, r15 0.98, r20 0.76, r30 0.45, r50 0.16.
- WaW (Der Riese, [T4]) also starts at 2.0. It clamps to 0.08 and *then* multiplies by 0.95, so the effective floor is about 0.076.
- **WaW Nacht starts at 3.0** ([T4n]).
- Between Rounds: `zombie_between_round_time = 10` s in both games.

## 5. Speed Tiers (walk/run/sprint)

Both games use the same per-zombie roll at spawn ([T5]/[T4] `set_run_speed`):

```
rand = randomintrange(base, base + 35)       // [base, base+34]
rand <= 35 → walk;  rand <= 70 → run;  else → sprint
```

- `base` starts at 1 in round 1. At the end of round N it becomes `N * 8`, so round N uses `(N-1) * 8` (`zombie_move_speed_multiplier = 8` in [T5csv] MEDIUM; WaW hard-codes `* 8`).
- The result is: round 1 all walkers, round 2 the first runners, round 6 the first sprinters, round 10 and later all sprinters.

| Round | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10+ |
|---|---|---|---|---|---|---|---|---|---|---|
| walk % | 100 | 80 | 57 | 34 | 11 | 0 | 0 | 0 | 0 | 0 |
| run % | 0 | 20 | 43 | 66 | 89 | 89 | 66 | 43 | 20 | 0 |
| sprint % | 0 | 0 | 0 | 0 | 0 | 11 | 34 | 57 | 80 | 100 |

- There is a last-zombie speed-up in BO1 ([T5] `zombie_speed_up`), in rounds 4–9 only. Once at most 4 zombies remain to spawn and at most 3 are alive, the last legged zombie is switched to a sprint animation.
- Crawlers (zombies that have lost their legs) are a separate state and are not part of the speed roll.

## 6. Points economy

BO1 values come from [T5csv]; the logic is in [T5] `_zombiemode_score.gsc`. Every award passes through `multiplier * round_up_score(points, 5)`, where `multiplier` is 2 during Double Points.

| Event | BO1 | WaW |
|---|---|---|
| Starting Points (solo) | **500** (`zombie_score_start_1p`) | 500 |
| Non-lethal hit | **10** (`zombie_score_damage_normal`/`_light`) | 10 (`zombie_score_damage 5`, rounded up to 10) |
| Kill, base | 50 | 50 |
| Kill bonus: head / helmet | +50 → **100** | same |
| Kill bonus: neck | +20 → **70** | same |
| Kill bonus: torso (upper/lower) | +10 → **60** | same |
| Kill: limbs / other | **50** | same |
| Melee (knife) kill | +80 → **130** | same |
| Burn kill | +10 → 60 | same |
| Plank rebuilt | **10** per board (default `cost = 10`) | 10 |
| Nuke | **400** to each player; nuked zombies give no kill points | 400 |
| Carpenter | **200** to each player | 200 |

- **The lethal hit gives kill points only, never hit points.** The `damage` handler sends dead zombies down the death path before any hit-point award ([T5] `_gameskill.gsc` `auto_adjust_enemy_death_detection`).
- During Insta-Kill, kill points are doubled for `MOD_UNKNOWN` deaths ([T5]).
- In BO1, an ADS hit is still 10. The `damage_ads` (×1.25) path is only used for flame damage.

**Per-Round cap on rebuild points**

At the start of each Round, `rebuild_barrier_cap_per_round = min(50 * round, 500)` ([T5]/[T4] `round_think`, "designed by prod DT#36173").

- A per-player counter adds 10 for each board, and the points are awarded only while `counter < cap`. The board that reaches the cap earns nothing.
- So each round can earn at most cap − 10: round 1 earns 40 (4 boards), round 5 earns 240, and round 10 or later earns 490.
- WaW difference: under Double Points, WaW doubles `cost` *before* adding it to the counter, so the cap fills twice as fast. BO1 adds the raw 10 and doubles only the award.
- One rebuild trigger repairs a board about once per second (`wait 1`) while Use is held. Speed Cola (`specialty_fastreload`) speeds up the board animation.

Other point rules:
- Death and down penalties in solo are effectively irrelevant: `penalty_downed 0.05`, `penalty_died 0.0`.
- Grenade restock: every surviving player is topped up to 2–4 lethal grenades at the start of each Round ([T5] `award_grenades_for_survivors`).

## 7. Barriers (windows)

- **Planks per window:** this comes from map data, since each `exterior_goal` targets its own `barrier_chunks`. The standard WaW/BO1 window has **6 boards**. *(The scripts do not fix this number. The figure is the community-standard value and was not verified against map ents.)*
- **Attack spots:** each window has 3 attack spots: centre, left and right ([T5] `get_attack_spot`, `attacking_spot_index 0/1/2`). Zombies with no free spot taunt (5% chance) and retry every 0.5 s.
- **Tear-down:**
  - Each zombie pulls the closest board that is not destroyed, one board at a time.
  - Tear duration is the length of the tear animation (`AnimScripted "tear_anim"` plus notetracks). There is no tuned timer in script, so the exact seconds per board come from animation data and are not available here.
  - The zombie enters once `all_chunks_destroyed`.
- **Attack through the window** ([T5] `should_attack_player_thru_boards`, `window_notetracks`):
  - After each board pull, a legged zombie checks for players within 90 units (2D).
  - If one is found, a 50% roll (`zombie_reachin_freq` 50) triggers a reach-through swipe.
  - The swipe damages a player only if they are within 90 units of the zombie (2D and height) **and** within 51 units of the window's rebuild trigger.
  - Damage is the zombie's `meleeDamage`, which is 60.
  - The check is skipped while any "unbroken" special barricade (glass or metal) remains.
- **Rebuild trigger:** a radius trigger of 96 units, 96 tall, at the window ([T5] `blocker_trigger_think`). Rebuilding is hold-Use, one board per second, 10 Points per board up to the cap.

## 8. Door / debris prices

- The script default is `cost = 1000` when the map gives no `zombie_cost` ([T5]/[T4] `_zombiemode_blockers.gsc`). Actual prices are set per map in the map entities, not in GSC, so the per-map figures below come from wikis and guides.
- In solo, a door is bought outright if `score >= zombie_cost` and the Points are deducted.

| Map | Prices | Source quality |
|---|---|---|
| Nacht der Untoten | 1000 everywhere: Help Room door, Upstairs debris, the Upstairs-to-Help debris | Wiki (via search summary) |
| Verrückt | 750–1000 doors and debris; the spawn-split door is opened by Power, not bought | [SegmentNext guide](https://segmentnext.com/call-of-dutyworld-at-war-nazi-zombies-verruckt-map-strategy-guide/) |
| Kino der Toten | 750 (lobby stairs doors), 1000 (alley and dressing-room steel doors), 1250 (later doors such as the Stage and Projector routes) | [bo1zombies.us](https://bo1zombies.us/maps/kino/), [GameFAQs thread](https://gamefaqs.gamespot.com/boards/960188-call-of-duty-black-ops/57125185) |

**Faithful range: 750 / 1000 / 1250**, rising with distance from spawn. The exact per-door Kino and Verrückt mapping was not verified against map entities.

## 9. Power-ups

**Drop rule** (BO1 [T5] `watch_for_drop` and `powerup_drop`; WaW [T4] is the same):

- **Points-based threshold:**
  - The first threshold is `players * start_points + 2000`, which is 2500 `score_total` in solo. `score_total` is lifetime Points, including the starting 500, so the first threshold is about 2000 earned.
  - Each time the summed `score_total` passes the threshold: `increment *= 1.14`, `threshold = current_total + increment`, and a drop flag is armed.
  - The next eligible zombie death drops the Power-up at the corpse, if it is inside the playable area.
- **Random drop:** each eligible death also has a **3%** chance (`randomint(100) <= 2`) of dropping even without the flag.
- **Per-Round cap:** at most **4** drops per Round (`zombie_powerup_drop_max_per_round`), counted by `powerup_drop_count` and reset at Round start.
- If the drop position is outside every `player_volume`, the drop is cancelled and the count refunded.

**Drop cycle and order** (`get_next_powerup`, `get_valid_powerup`):

- The included Power-ups form an array, shuffled with `array_randomize` once at init, and are dealt in order.
- When the array is exhausted, the index resets and it is reshuffled. It is a shuffle-bag, not an independent roll.
- Skip rules:
  - **Carpenter** is skipped unless at least 5 windows are fully destroyed.
  - **Fire Sale** is skipped until the Mystery Box has moved at least once, or while one is active (BO1 only).
- **Map sets:** BO1 Kino includes nuke, insta_kill, double_points, full_ammo, carpenter and fire_sale ([T5] `zombie_theater.gsc`). BO1 Nacht includes the first five, with no Fire Sale ([T5] `zombie_cod5_prototype.gsc`). WaW has the same five base drops and no Fire Sale ([T4]).

**Pickup lifetime:** the Power-up sits for 15 s, then blinks for 11.5 s (15 × 0.5 s, 10 × 0.25 s, 15 × 0.1 s), then despawns. That is about **26.5 s** in total ([T5]/[T4] `powerup_timeout`).

| Power-up | Effect | Duration |
|---|---|---|
| Max Ammo | `GiveMaxAmmo` on every weapon of every non-downed player; clip is not refilled, only reserve (BO1) | instant |
| Insta-Kill | any damage kills; kill points doubled for `MOD_UNKNOWN` | **30 s** (re-pickup restarts the timer) |
| Double Points | `zombie_point_scalar = 2` for all point awards | **30 s** (re-pickup restarts the timer) |
| Nuke (Kaboom) | 0.5 s after pickup, marks all zombies, then kills them one by one with 0.1–0.7 s random gaps; no kill points; **+400** each player (BO1 goes through `player_add_points`, so x2 applies; WaW adds the raw 400) | instant |
| Carpenter | repairs every window; +**200** each player (BO1: x2 applies; WaW: raw); BO1 instantly fixes windows far from players (more than 750 units) and animates the near ones | instant |
| Fire Sale (BO1) | Box appears at every location at a reduced price (10 per community sources; the price was not read from script) | 30 s (`zombie_powerup_fire_sale_time`) |

## 10. Player Health and damage taken

BO1 ([T5] `player_damage_override`, `_zombiemode_spawner.gsc` L244, [T5csv]):

- Player max health is **100**. Each zombie swipe deals **60** (`self.meleeDamage = 60`), and that replaces whatever damage the engine computed.
- **Without Juggernog:** 2 hits down you (60 → 120 > 100).
- **With Juggernog:** max health is 250 (`zombie_perk_juggernaut_health`), so **5 hits** down you (4 × 60 = 240 < 250).
- **Per-zombie hit cooldown:** after a zombie hits you, *that same zombie* is ignored for **0.4 s** (`ignore_enemy_timer`). Different zombies can hit back-to-back.
- **Explosives:** your own projectile or grenade damage is capped at 75 while health is above 75. PhD Flopper removes it.
- **Regen:** handled by `_gameskill.gsc` at skill 1 ("normal", difficulty fraction 0.75). The regen delay is **about 2.4 s** after the last hit (`playerHealth_RegularRegenDelay`, 2400 ms at normal).
  - Normal skill also sets brief invulnerability windows after hits: `invulTime_preShield` about 0.35 s, `invulTime_onShield` about 0.5 s, `invulTime_postShield` about 0.3 s. *(These values are inferred from the interpolation table at fraction 0.75 and were not measured in game.)*
- **Solo death:** `starting_lives` = 0 in MEDIUM, so a down with no Quick Revive is Game Over. (BO1 solo Quick Revive grants a self-revive.)

WaW ([T4]):

- Max health is 100. Juggernog sets it to **160** (`zombie_perk_juggernaut_health 160`), so WaW's Jugg is much weaker than BO1's.
- The script does **not** override zombie swipe damage. It comes from engine and aitype melee data. The 0.4 s same-attacker cooldown is the same as BO1.
- **Unverified:** with the commonly cited 50 per swipe, it would take 2 hits to down without Jugg and 4 with it. The exact WaW swipe damage was not confirmed from script.

---

## Not verified / caveats

- **Planks per window (6):** this lives in map data, not script. The figure is community-standard.
- **Seconds per board torn down:** set by animation length; not available from script.
- **Per-door prices:** Nacht, Verrückt and Kino door prices come from wikis and guides, not map entities. Fandom pages returned HTTP 402 to the fetcher, so the figures came via search summaries and third-party guides.
- **WaW swipe damage and Jugg hit count:** engine-side, not confirmed.
- **Regen and invulnerability timings:** derived from the `_gameskill` table at the assumed normal fraction, not measured.
- **Source provenance:** the plutoniummod dumps are community-dumped stock scripts. The WaW mod-tools repo is a mirror of Treyarch's official release. The CSV comes from a community table dump that matches the JTAG T5 raw-file dump in size.
