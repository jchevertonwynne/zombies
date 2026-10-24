# Weapons, Mystery Box, Perks, Power and Pack-a-Punch: WaW/BO1 numbers

Research for ticket `.scratch/zombies-v2/issues/02-weapons-box-perks-pap-numbers.md`. BO1 is the baseline. WaW differences are called out where they matter.

## Sources and trust

| Tag | Source | Trust |
|---|---|---|
| **[BO1-GSC]** | SyndiShanX/COD-GSC-Source, `BO1-GSC/maps/*.gsc`: decompiled BO1 PC scripts (final patch). <https://github.com/SyndiShanX/COD-GSC-Source> | Primary (game logic) |
| **[WAW-GSC]** | Same repo, `WAW-GSC/maps/*.gsc`: WaW scripts | Primary |
| **[T5-CSV]** | JTAG7371/T5-RawFile-Dump, `mp/zombiemode.csv`: BO1 rawfile cost/difficulty table that the GSC reads with `TableLookUp`. <https://github.com/JTAG7371/T5-RawFile-Dump> | Primary (data) |
| **[WF]** | `weapons/sp/<name>` weapon files at their *first* commit in Jbleezy/BO1-Reimagined (<https://github.com/Jbleezy/BO1-Reimagined>). BO1's weapon files are not rawfiles; this mod ships them | Secondary. It is a mod, so values may already be tuned. Only treat a value as solid when it matches [WIKI] |
| **[WIKI]** | callofduty.fandom.com wikitext (via `api.php?action=parse`): "Upgraded Weapons/Black Ops", "Kino der Toten", "Verrückt", "Mystery Box", perk pages, "Pack-a-Punch Machine", "Perk-a-Cola" | Well-sourced wiki |

Key files: `_zombiemode_weapons.gsc` (wall buys, box), `_zombiemode_perks.gsc` (perks, PaP), `_zombiemode.gsc` (solo revive), `zombie_theater.gsc` (Kino include list, power), `zombie_theater_magic_box.gsc`.

---

## 1. Wall Weapons (Kino der Toten roster, BO1)

### Pricing rules [BO1-GSC] `_zombiemode_weapons.gsc`
- **Cost** is the hard-coded `add_zombie_weapon` value, overridden by `mp/zombiemode.csv` when the CSV has a row. The value is rounded up to the nearest 10.
- **Ammo cost** defaults to `round_up_to_ten(cost * 0.5)`, so ammo costs half the weapon price.
- **Ammo for a Pack-a-Punched copy of the wall weapon costs 4500** (`if(player has_upgrade(...)) ammo_cost = 4500;`).
- Kino's wall weapons are included with `include_weapon(name, false, true)`: not in the box, and they count toward the "Collector" achievement (`zombie_theater.gsc`).
- WaW uses the same half-price ammo formula [WAW-GSC]. Grenades: ammo cost = cost. WaW has no upgraded-ammo rule because PaP only arrives with Der Riese.

### Kino wall roster [WIKI Kino der Toten] + prices [T5-CSV]/[BO1-GSC]

Stats are BO1 Zombies values. Damage is max–min falloff and RPM comes from `fireTime`. Reserve is the carried max excluding the magazine. Where [WF] and [WIKI] disagree, both are shown.

| Weapon (BO1 name) | Internal | Wall cost | Ammo | Damage | Mag | Reserve | Fire | RPM | Reload (tac / empty, s) | Head mult |
|---|---|---|---|---|---|---|---|---|---|---|
| M1911 (start pistol) | `m1911_zm` | n/a (start weapon; `add_limited_weapon(...,0)` keeps it out of the box) | n/a | 20 | 8 | 80 (start 32) | semi | 625 cap | 1.63 / 1.8 | x3.5 |
| Olympia | `rottweil72_zm` | 500 (Lobby down) | 250 | **[WIKI] 80×8 pellets**; [WF] 180×8 (likely mod-tuned) | 2 | 38 | break-action | 212 | 3.3 / 3.9 | – |
| M14 | `m14_zm` | 500 (Lobby up) | 250 | 105–80 | 8 | 96 | semi | 625 cap | 2.4 / 3.45 | x3 |
| PM63 | `pm63_zm` | 1000 (Upper Hall) | 500 | 100–60 | 20 | **[WIKI] 100**; [WF] 120 | auto | 937 | 2.05 / 2.85 | x4 |
| MPL | `mpl_zm` | 1000 (Lower Hall) | 500 | 100–60 | 24 | 120 | auto | 937 | 2.25 / 3.0 | x4 |
| MP5K | `mp5k_zm` | 1000 (Dressing Room) | 500 | 100–50 | 30 | 120 | auto | 750 | 2.9 / 3.2 | x4 |
| MP40 | `mp40_zm` | 1000 (Foyer down) | 500 | 100–60 | 32 | 192 | auto | 535 | 2.3 / 2.9 | x4 |
| AK-74u | `ak74u_zm` | 1200 (Alley) | 600 | 120–80 | 20 | 160 | auto | 750 | 2.1 / 2.8 | x4 |
| M16 | `m16_zm` | 1200 (Stage) | 600 | 100–70 | 30 | **[WIKI] 120**; [WF] 150 | 3-rd burst | 937 in burst (~459 eff.) | 2.03 / 2.36 | x4 |
| Stakeout | `ithaca_zm` | 1500 (Foyer up) | 750 | 160×8 pellets | 6 | 54 | pump | 63 | 0.567 per shell | – |
| Bowie Knife | – | 3000 (Theater) | – | melee upgrade | | | | | | |
| Claymore | `claymore_zm` | 1000 (Stage) | – | placeable | | | | | | |
| Frag grenades | `frag_grenade_zm` | 250 (Projector) | 250 | | | | | | | |

Damage falloff ranges [WF] (inches): SMGs 800→1800; M14 1400→2500; M16 1600→3000; Stakeout 256→800; Olympia 300→900.

### Box/other reference weapons
| Weapon | Cost | Damage | Mag | Reserve | Notes |
|---|---|---|---|---|---|
| Python (box) | 2200 (CSV) | 1000–300 | 6 | 84 | 0.65 s per round reload |
| Ray Gun (box) | 10000 (CSV, nominal) | [WIKI] 1000 direct + 1500–300 splash; [WF] 2500 inner / 1250 outer, 64-unit radius | 20 | 160 | 181 RPM, 3.0 s reload |

### WaW wall prices for comparison [WAW-GSC] `_zombiemode_weapons.gsc`, [WIKI Verrückt]
Kar98k/Springfield 200 · Gewehr 43 / M1 Garand / M1 Carbine 600 · MP40 1000 · Thompson / STG-44 / Double-Barrel 1200 · Trench Gun (`zombie_shotgun`) 1500 · BAR 1800 (bipod 2500) · Bouncing Betty 1000 · grenades 250 · Ray Gun 10000.

---

## 2. Mystery Box (BO1) [BO1-GSC] `_zombiemode_weapons.gsc`

- **Cost: 950** (`old_cost = 950` unless the map's chest sets `zombie_cost`). During Fire Sale it costs 10.
- **Spin:** 40 cycles at 0.05/0.1/0.2/0.3 s steps, about 2.85 s of visible cycling. Every cycle except the last shows a uniformly random weapon. Only the last cycle uses the weighted pick.
- **Pickup window: 12 s** (`treasure_chest_timeout() wait(12)`; the weapon model sinks back over `putBackTime = 12`). The wiki says "ten seconds", but the code says 12. After a grab or timeout the lid closes, and the box is usable again after a 3 s wait.
- **The 950 is spent either way.** If you don't take the weapon you get no refund.

### Weapon pool and weights
- The pool is every `include_weapon` entry with `in_box = true`. Each entry gets `weighting_func()` copies in a list, then one is picked uniformly. **The default weight is 1 for every weapon.** In BO1 no shipped map (Kino included) passes a custom weighting func, so the Ray Gun and the Monkey Bomb are **uniform** with the rest. The unused helpers `default_cymbal_monkey_weighting_func` (3 below round 10, 5 from round 10) and `default_tesla_weighting_func` exist but aren't wired.
- **Excluded from the pick:** any weapon the player already holds, including its upgraded form (`has_weapon_or_upgrade`). Also excluded: "limited weapons" at their cap, counting players' inventories, PaP contents, other boxes and powerups. Kino caps: Thundergun 1, Crossbow 1, Ballistic Knife 1; M1911 0, so it never appears in the box.
- **Kino box pool (23 entries):** CZ75, CZ75 Dual Wield, Python, Spectre, G11, FAMAS, AUG, Commando, Galil, FN FAL, HS-10, SPAS-12, Dragunov, L96A1, RPK, HK21, M72 LAW, China Lake, Monkey Bomb, Ray Gun, Thundergun, Crossbow, Ballistic Knife. So the Ray Gun is roughly 1/23 ≈ 4.3% per pull, and slightly more when you hold pool weapons.

### Teddy Bear rule (when the box moves)
`level.chest_accessed` counts *completed* uses: weapon grabbed or timed out, and Fire Sale uses don't count. It resets to 0 on a move. The check runs each spin, with `random = RandomInt(100)` and a teddy when `chance_of_joker > random`:

| Location | `chest_accessed` before this spin | Teddy chance |
|---|---|---|
| any | 0–3 | 0% (`chest_min_move_usage = 4`) |
| first location (`chest_moves == 0`) | 4–7 | 15% |
| first location | ≥ 8 | **100%** |
| later locations | 4–7 | 15% |
| later locations | 8–12 | 30% |
| later locations | ≥ 13 | 50% |

So pulls 1–4 at a location are always safe. At the first location the box leaves by the 9th pull at the latest. Later locations have no hard cap.

- **Teddy outcome:** the spinner's 950 is refunded (`add_to_player_score(user_cost)`) unless Fire Sale was active. After 0.5 s the teddy rises and flies away over about 4 s. The box lid closes and the box shakes upward for 5 s, then "poofs". The new location appears **12 s later** (`post_selection_wait_duration = 7 + 5` when there is no fire sale). A rubble/teddy prop marks each empty spawn.
- **Next location:** first, any chest whose `script_noteworthy` contains `move<N+1>` (scripted ordering). Otherwise the next index in the array that was randomised at game start. When the list is exhausted it is re-randomised, never repeating the current spot.
- **Start location:** Kino uses `level.random_pandora_box_start = true`, so it starts at a random chest not flagged `start_exclude`. Other maps use the chest tagged `start_chest`.
- **Kino has 9 locations** (`zombie_theater_magic_box.gsc`): start, foyer, crematorium, alleyway, control, stage, dressing, dining, theater. Power only lights the map indicator that shows where the box is; the box itself works without power.

### WaW differences [WAW-GSC]
- Nacht der Untoten never moves the box (`level.script != "nazi_zombie_prototype"`; BO1's Nacht port sets `magic_chest_movable 0`).
- On WaW maps other than Shi No Numa, the chance is `chest_accessed + 20` once `chest_accessed ≥ 4` (≈25% and rising 1% per pull, using `random <= chance`). The graduated 15/30/50% table above appears **only for `nazi_zombie_sumpf`** in WaW, and BO1 made it the default.
- WaW refunds a hard-coded 950 on teddy.

---

## 3. Perks [BO1-GSC] `_zombiemode_perks.gsc`

| Perk | Internal | BO1 cost | WaW cost | Effect |
|---|---|---|---|---|
| Juggernog | `specialty_armorvest` | 2500 | 2500 | Max health 100 → **250** (`zombie_perk_juggernaut_health = 250`, [T5-CSV]); reset to 100 on loss. [WIKI]: down on the 5th zombie hit instead of the 3rd |
| Speed Cola | `specialty_fastreload` | 3000 | 3000 | Reload time −50% (engine perk; [WIKI]). Also speeds the barrier-rebuild animation only |
| Double Tap Root Beer (1.0) | `specialty_rof` | 2000 | 2000 | Fire rate +33% (engine dvar `perk_weapRateMultiplier 0.75`, i.e. fire interval ×0.75, seen in a T5 toolconfig ddl; [WIKI] "33%"). **No** extra damage, which arrived with 2.0 in BO2 |
| Quick Revive | `specialty_quickrevive` | **500 solo** / 1500 co-op | 1500 | Co-op: revive others 2× faster. Solo BO1: self-revive (below) |

- **Power:** all perk machines show "need power" until the map's notify (`juggernog_on`, `sleight_on`, `doubletap_on`, `revive_on`). **Exception: solo Quick Revive is powered from the start** (machine model set to "on" immediately when `players.size == 1`).
- **Can't buy** if you already own the perk, can't afford it, are drinking, switching weapons, throwing a grenade, or are in a revive trigger.
- **Perk limit (BO1): 4.** `if(player.num_perks >= 4)` plays a deny sound. This is the final-patch PC code. [WIKI] says BO1–BO3 had a 4-perk limit, and that it only mattered from Ascension on (more than 4 machines). Kino has exactly 4 machines, so it never comes up. WaW has no limit in code and only 4 perks.
- **Losing perks:** every perk is removed on going down (`perk_think` waits on `player_downed`) and must be rebought. Jugg's loss resets max health to 100.

### Solo Quick Revive (BO1) [BO1-GSC] `_zombiemode_perks.gsc`, `_zombiemode.gsc`
- Solo is detected when there is 1 player: `flag solo_game`, `players[0].lives = 0`, and the last-stand pistol becomes **`m1911_upgraded_zm` (Mustang & Sally)**.
- Buying it costs **500**, works pre-power, sets `self.lives = 1` and increments `level.solo_lives_given`.
- While you hold a life, the machine trigger is turned off. It turns back on once the life is consumed, so you can rebuy.
- **Limit 3 purchases.** On the 3rd purchase `flag solo_revive` is set, and the machine waits 2 s, rises 40 units over 3 s, shakes, "poofs" and is deleted with its clip. Its spot becomes walkable. After that no further solo revives are possible from the machine.
- **When downed with a life:** you enter last stand with Mustang & Sally (grenades stashed, then restored). After **10 s** (`solo_revive_time = 10.0`) you auto-revive, `lives--`, and all perks are lost, Quick Revive included (solo QR is never retained). Without a life, a solo down means `end_game`.
- [WIKI] adds that zombies move away from the player during the self-revive and that no points are earned while down. I did not verify either in the GSC.
- WaW: Quick Revive is 1500 and useless in solo. No self-revive.

---

## 4. Power (Kino) [BO1-GSC] `zombie_theater.gsc`, `_zombiemode_blockers.gsc`, `_zombiemode_traps.gsc`
Flipping `use_elec_switch` sets `flag power_on`, which:
- turns on the Quick Revive (co-op), Juggernog, Speed Cola and Double Tap machines, and **Pack-a-Punch** (`Pack_A_Punch_on`)
- initialises the **teleporter** (`zombie_theater_teleporter::teleporter_init`), which is how Kino reaches PaP
- enables traps (`ZOMBIE_NEED_POWER` until `power_on`) and opens `electric_door` / `electric_buyable_door` blockers
- links extra zone adjacencies (theater↔foyer2, theater↔stage gated by `power_on`)
- lights the box-location indicator and chandeliers (cosmetic), and starts Kino's crawler ("quad") waves (`quads_per_round = 4 × players`)

It is a one-shot switch: the trigger is deleted after use. Not gated by power: wall weapons, doors/debris (except electric doors), the Mystery Box, and solo Quick Revive.

---

## 5. Pack-a-Punch (BO1) [BO1-GSC] `_zombiemode_perks.gsc`, `_zombiemode_weapons.gsc`

- **Cost 5000** (`vending_weapon_upgrade_cost`); 1000 during a Bonfire Sale (a later-map power-up).
- **Requires power** (`level waittill("Pack_A_Punch_on")`).
- **Refused if** the current weapon is already upgraded (`is_weapon_upgraded`), so **there is no repack in BO1**. Repack at 2000 arrived in BO2 [WIKI]. Also refused if the weapon has no upgrade entry, or the player is in last stand, throwing a grenade or switching weapons.
- **Sequence:** pay, knuckle-crack animation, the weapon goes into the machine, about 3 s of processing, then the upgraded weapon slides out. **Pickup window 15 s** (`level.packapunch_timeout = 15`). On timeout the weapon is **lost** and the 5000 is not refunded. The weapon is taken from the player when inserted.
- The player receives the upgraded weapon with **full start ammo** (`GiveStartAmmo`).
- **Camo:** every upgraded weapon gets **camo index 15** (the PaP camo), a random lens (0–5), a random reticle (0–20) and a random reticle colour (0–5). A few reticles force a colour, and the FAMAS always gets the smiley reticle. The options are cached per player per weapon.
- Ammo for an upgraded weapon from its wall chalk costs 4500 (see §1).
- WaW: PaP debuts in Der Riese at 5000 [WIKI]. The WaW GSC in this dump is the pre-Der Riese common script, so I did not code-verify the WaW PaP flow.

### Upgraded roster (BO1) [WIKI "Upgraded Weapons/Black Ops"], cross-checked against [WF]

| Base | Upgraded name | Damage | Mag | Reserve | Notes |
|---|---|---|---|---|---|
| M1911 | Mustang & Sally | 1000 + 1200 splash (wiki) / [WF] 2500 inner, 1000 outer, r=200 | 6 (dual wield) | 50 | fires grenades, 300 RPM |
| Olympia | Hades | 200×12 | 2 | 60 | incendiary rounds, reload 2.0/2.5 |
| M14 | Mnesia | 200–150 | 16 | 192 | head x6 |
| PM63 | Tokyo & Rose | 140–90 | 25 per gun (dual) | 225 (wiki) / [WF] 250 | reload 3.0/3.4 (wiki) |
| MPL | MPL-LF | 140–90 | 40 | 200 | |
| MP5K | MP115 Kollider | 140–80 | 40 | 200 | |
| MP40 | The Afterburner | 200–120 | 64 | 192 | 600 RPM (wiki) |
| AK-74u | AK-74fu2 | 190–140 | 40 | 280 | |
| M16 | Skullcrusher | 150–100 | 30 | 270 | full auto, 937 RPM; M203 GL 700 impact, 400–75 splash, 8+1 rounds |
| Stakeout | Raid | 300×8 | 10 | 60 | reloads 2 shells at once |
| Python | Cobra | 1000–600 | 12 | 96 | |
| Ray Gun | Porter's X2 Ray Gun | 1000 + 2000–300 splash | 40 | 200 | |

Upgraded head multipliers are generally x5 for SMGs and rifles [WIKI].

---

## Not verified / caveats
- **Weapon stats** come from a modded weapon-file set ([WF]) plus the wiki. They agree on most values. The conflicts are Olympia damage (80 vs 180), M16 reserve (120 vs 150), PM63 reserve (100 vs 120) and Ray Gun/Mustang & Sally damage figures. A clean extraction of BO1 `weapons/sp` from the original FastFiles would settle them.
- **Speed Cola −50% / Double Tap +33%** are engine dvars and don't appear in the GSC. The Double Tap `0.75` multiplier was only seen in a T5 toolconfig file, and the Speed Cola value rests on the wiki.
- **Bowie Knife, claymore and grenade** numbers were not researched beyond price.
- I did not check whether the BO1 4-perk limit existed at launch or was patched in. The dump is the final PC patch.
- **WaW PaP internals** and **WaW Jugg on Verrückt/Shi No Numa**: the wiki says these maps used a different regen-based Jugg rather than 250 HP. Not code-verified.
