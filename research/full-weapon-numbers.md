# Full weapon, handling and equipment numbers (BO1 Zombies)

Research for [#17](https://github.com/jchevertonwynne/zombies/issues/17). It extends `research/weapons-box-perks-pap.md` (branch `research/weapons-box-perks-pap`, issue #3), which covered the Kino wall roster, the box, perks, power and PaP. BO1 is the baseline. WaW is used only where BO1 has no answer, and every WaW value is labelled.

## Sources and trust

| Tag | Source | Trust |
|---|---|---|
| **[BO1-GSC]** | SyndiShanX/COD-GSC-Source `BO1-GSC/maps/*.gsc`: decompiled BO1 PC scripts (final patch). Files: `_zombiemode.gsc`, `_zombiemode_claymore.gsc`, `_zombiemode_weap_cymbal_monkey.gsc`, `_zombiemode_weap_crossbow.gsc`, `_zombiemode_weap_thundergun.gsc`, `_zombiemode_bowie.gsc`, `_ballistic_knife.gsc`, `_zombiemode_powerups.gsc`, `_zombiemode_weapons.gsc`, `zombie_theater.gsc` | Primary (game logic) |
| **[WAW-GSC]** | Same repo, `WAW-GSC/maps/*.gsc`. This is the pre-Der Riese common script | Primary |
| **[WAW-WF]** | CallOfDutyModding/call_of_duty_world_at_war_mod_tools `raw/weapons/sp/*` and `raw/info/bullet_penetration_sp`: Treyarch's **official** WaW mod-tools raw weapon files | Primary (data), WaW only |
| **[WF]** | Jbleezy/BO1-Reimagined `weapons/sp/<name>`, each file **at the commit that first added it** (2017-08 `5a6a793` to 2018-08 `ab1d8bc`). BO1 has no official raw weapon release, and this mod is the only near-complete set | **Secondary.** Some first-commit files are definitely already tuned (see the conflicts below). Treat a [WF] value as solid only when [WIKI] or [WAW-WF] agrees with it |
| **[WIKI]** | callofduty.fandom.com wikitext through `api.php?action=parse` (weapon infoboxes and PaPComparison templates): CZ75, Python, Spectre (weapon), G11, FAMAS, AUG (rifle), Commando (weapon), Galil, FAL, HS-10, SPAS-12, Dragunov, L96A1, RPK, HK21, M72 LAW, China Lake, Crossbow, Ballistic Knife, Thundergun, Monkey Bomb, Bowie Knife, MP40, Frag Grenade | Well-sourced wiki |
| **[T5-CSV]** | JTAG7371/T5-RawFile-Dump `mp/zombiemode.csv` | Primary (data) |

Conventions:
- Units are inches (Quake units) and seconds.
- RPM = 60 / `fireTime`. Semi-auto RPM is a cap.
- Reserve is the carried ammo excluding the magazine. In [WF], `ammoCountClipRelative 1` means `maxAmmo` counts magazines, so reserve = maxAmmo × clip.
- Head mult is the weapon-file `locHead` multiplier. BO1 script adds **no** extra head multiplier for damage: `actor_damage_override` only applies mutators. Headshots change points only.

---

## 1. Box weapon stats (Kino pool, plus the WaW-port Trench Gun and MP40)

Damage is max–min, with falloff ranges in inches. Where [WF] and [WIKI] disagree, the [WIKI] value is used and the [WF] value is shown in brackets.

| Weapon | Internal | Class / fire | Damage | Falloff (in) | Head mult | Mag | Reserve | RPM | Reload tac / empty (s) | Penetrate type |
|---|---|---|---|---|---|---|---|---|---|---|
| CZ75 | `cz75_zm` | pistol, semi | 150–100 | 700→1800 | x3.5 | 15 | 135 | 625 cap | 1.63 / 2.013 | small |
| CZ75 Dual Wield | `cz75dw_zm` (+`cz75lh_zm`) | pistol, semi ×2 | 150–100 | 700→1800 | x3.5 | 12+12 | 228 | 625 each | 1.9 / 2.2 | small |
| Python | `python_zm` | revolver | 1000–300 | 384→1200 | x1.5 | 6 | 84 | 625 cap | 1.6 start + 0.65/round; 1.0 empty | medium |
| Spectre | `spectre_zm` | SMG, auto | 90–50 | 800→1800 | x4 | 30 | **120** [WF 240] | 937 | 2.3 / 3.0 | small |
| G11 | `g11_lps_zm` | AR, 3-rd burst | 100–70 | 1600→3000 | x4 | 48 | **144** [WF 192] | 1250 in-burst | 2.7 / 3.85 | medium |
| FAMAS | `famas_zm` | AR, auto | 100–70 | 1600→3000 | x4 | 30 | **150** [WF 300] | 937 | 2.5 / 3.3 | medium |
| AUG | `aug_acog_zm` | AR, auto | 140–90 | 1600→3000 | x4 | 30 | 270 | 937 | 2.6 / 3.05 | medium |
| Commando | `commando_zm` | AR, auto | 150–100 | 1600→3000 | x4 | 30 | 270 | 750 | 2.05 / 2.55 | medium |
| Galil | `galil_zm` | AR, auto | 150–100 | 1600→3000 | x4 | 35 | 315 | 750 | 2.8 / 3.8 | medium |
| FN FAL | `fnfal_zm` | battle rifle, semi | 160–130 | 1600→3000 | x4 | 20 | 180 | 625 cap | 2.5 / 3.1 | medium |
| HS-10 | `hs10_zm` | shotgun, semi | 160–15 ×8 pellets | 256→800 | x1 | 6 | 36 | 341 | 1.35 start, 0.65/shell, 1.0 end | small |
| SPAS-12 | `spas_zm` | shotgun, semi | 160–15 ×8 | 256→800 | x1 | 8 | 32 | 312 | 1.5 start, 0.567/shell, 1.0 end | small |
| Dragunov | `dragunov_zm` | sniper, semi | 300 flat | – | x10 head, x5 neck, x1.5 torso/arms | 10 | 40 | 234 | 2.95 / 3.75 | large |
| L96A1 | `l96a1_zm` | sniper, bolt | 500 flat | – | x10 head, x5 neck, x1.5 torso/arms | 5 | 45 | bolt (1.0 s rechamber) | 3.3 / 3.5 | large |
| RPK | `rpk_zm` | LMG, auto | 130–90 | 1024→2400 | x3 | 100 | 400 | 750 | 4.0 / 5.5 | large |
| HK21 | `hk21_zm` | LMG, auto | 150–110 | 1024→2400 | x3 | 125 | 500 | 536 | 3.75 / 4.75 | large |
| M72 LAW | `m72_law_zm` | rocket | 2000 impact + splash (see §4) | – | – | 1 | 20 | – | 3.0 | – |
| China Lake | `china_lake_zm` | grenade launcher, pump | 600 impact + 600–75 splash r200 | – | – | 2 | 20 | rechamber 2.05 | 2.0 start + ~1/grenade [WIKI] | – |
| Crossbow | `crossbow_explosive_zm` | bolt | 750 impact + bolt blast (§5) | – | – | 1 | 12 | – | 2.75 | – |
| Ballistic Knife | `knife_ballistic_zm` | spring knife | 500 projectile / 500 melee | – | x1.5 | 1 | 4 | – | 1.5 | – |
| Thundergun | `thundergun_zm` | wonder weapon | scripted (§6) | – | – | 2 | 12 | 63 (0.95 s) | 2.1 / 2.0 | – |
| Ray Gun | `ray_gun_zm` | wonder weapon | 1000 impact + 1500–300 splash r64 (§4) | – | – | 20 | 160 | 181 | 3.0 | – |
| Monkey Bomb | `zombie_cymbal_monkey` | tactical | §5 | | | | | | | |
| **WaW ports** | | | | | | | | | | |
| Trench Gun | `zombie_shotgun` | shotgun, pump | 160–15 ×8 | 256→800 | x1 | 6 | 60 | 212 | 0.9 start, 0.6/shell, 0.95 end | medium |
| MP40 | `mp40_zm` | SMG, auto | 100–60 | 800→1800 | x4 | 32 | 192 | 536 | 2.3 / 2.9 | small |

Notes:
- Agreement with [WIKI]: damage matches for every row the wiki lists (CZ75, Python, Spectre, G11, FAMAS, AUG, Commando, Galil, HS-10, SPAS, Dragunov incl. multipliers, China Lake, Crossbow, Ballistic Knife, MP40). Reserves match except Spectre, G11 and FAMAS.
- **Trench Gun**: [WF] matches the official [WAW-WF] `zombie_shotgun` on damage, pellets, ranges, mag, reserve (60), fire time, reloads and spreads. It is a straight WaW port, used only on the WaW remaster maps, not Kino.
- **MP40**: the BO1 `mp40_zm` keeps the WaW damage (100–60, 32+192), but its handling differs from WaW `zombie_mp40`: hip 2→5 vs 1.5→6, raise 0.66 vs 0.35.
- Shotgun "min damage 15" applies per pellet beyond 800 in.
- The [WF] FN FAL has head x4. [WIKI] doesn't give the BO1 Zombies FAL damage (the infobox shows MP/BR only), so 160–130 is [WF] only.
- L96A1 damage 500 is [WF] only. [WIKI] says only that the Dragunov "does slightly less damage than the L96A1".

---

## 2. Handling per weapon / class [WF]

Spread values are in engine spread units, which are degrees of cone. `hipSpreadStandMin → hipSpreadMax` is the bloom range. `FireAdd` is the bloom added per shot, and spread decays at `hipSpreadDecayRate` per second. ADS spread 0 means a perfectly accurate ADS shot. `adsZoomFov` is the view FOV while aimed, against the base `cg_fov 65` set in `_zombiemode.gsc`.

| Class (weapons) | Hip stand min→max | Ducked min | FireAdd | ADS spread | ADS FOV | ADS in/out (s) | Raise / drop (s) | Quick raise / drop | First raise | Sprint-out | Move scale |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Pistols: M1911, CZ75 | 3→6 | 2.5 | 1.0 | 0 | 65 (no zoom) | 0.15 | 0.55 / 0.45 | 0.25 / 0.25 | 0.9 | 0.3 | 1.1 (CZ 1.0) |
| Python | 2→4 | 1.5 | 1.0 | 0 | 65 | 0.15 | 0.3 / 0.3 | 0.25 / 0.25 | 2.0 | 0.3 | 1.1 |
| CZ75 Dual Wield (no ADS) | 4→7 | 3.5 | 1.0 | – | – | – | 0.3 / 0.3 | 0.3 / 0.3 | 1.0 | 0.45 | 1.1 |
| SMGs: Spectre, PM63, MPL, MP5K, AK-74u, MP40 | 2→5 | 1.75 | 0.52 | 0 | 50 (PM63 55) | 0.2 | 0.65–0.85 / 0.3–0.7 | 0.25–0.65 / 0.25 | 0.66–1.4 | 0.3 (MP40 0.2) | 1.0–1.1 |
| ARs: FAMAS, AUG, Commando, Galil, M16 | 3→7 | 2.5 | 0.6 | 0 | 50 | 0.25 (AUG 0.35) | 0.7–0.9 / 0.45–0.75 | 0.5–0.75 / 0.25 | 0.75–1.25 | 0.3 | 0.95 |
| G11 (scope) | 2→5 | 1.75 | 0.6 | 0 | 30 | 0.3 / 0.6 | 0.7 / 0.55 | 0.5 / 0.25 | 1.75 | 0.3 | 0.95 |
| FN FAL, M14 | 5→9 (M14 3→7) | 4.5 | 0.6 (M14 1.0) | 0 | 50 | 0.25 | 0.8–0.85 / 0.6–0.7 | 0.45–0.7 / 0.25 | 0.8–1.1 | 0.3 | 0.95 |
| Shotguns: HS-10, SPAS, Stakeout | 4–5→7–8 | 4–5 | 0.6 | **4.25** (Stakeout 3.0) | 50–55 | 0.2 | 0.75–0.8 / 0.55–0.75 | 0.5 / 0.25 | 1.15–1.6 | 0.3 | 1.0 |
| Olympia, Trench Gun | 5→8 / 4→4 | | | **5.5** | 50 / 55 | 0.2–0.29 | 0.75 / 0.55; 0.6 / 0.38 | | | 0.3 / 0.2 | 1.0 |
| Snipers: Dragunov, L96A1 | **10→15** | 9.5 | 1.0 | 0 | 15 / 30 | 0.4 / 0.6 | 1.15–1.3 / 0.85–1.0 | 0.75–0.85 / 0.25 | 1.5–1.7 | 0.3 | 0.95 |
| LMGs: RPK, HK21 | 2.5→7 | 2.0 | 0.6 | 0 | 50 | 0.35 | 0.95 / 0.6–0.7 | 0.65–0.75 / 0.35 | 1.75–1.9 | **0.45** | 0.85–0.875 |
| Launchers: M72 LAW, China Lake | 5→6 | 3.5 | 0.4 | 1.7 | 55 / 80 | 0.5 | 1.1–1.3 / 1.0–1.1 | 0.65–0.75 / 0.25 | 1.4–1.5 | 0.3 / 0.6 | **0.8** |
| Ray Gun | 1→2 | 1.0 | 1.0 | 0 | 65 | 0.245 | 0.8 / 0.8 | 0.25 / 0.25 | 2.4 | 0.2 | 1.0 |
| Thundergun | 2→4 | 1.0 | 1.0 | 0 | 65 | 0.4 / 0.45 | 1.1 / 0.83 | 0.76 / 0.25 | 1.5 | 0.3 | 1.0 |
| Crossbow | 2→4 | 1.5 | 1.0 | 0 | 55 | 0.25 | 0.9 / 0.75 | 0.8 / 0.3 | 0.9 | 0.3 | 1.0 |
| Ballistic Knife | 0 | 0 | 0 | 0 | 55 | 0.25 | 0.35 / 0.35 | 0.25 / 0.25 | 1.0 | 0.3 | 1.1 |

- **Switch time** = old weapon `dropTime` + new weapon `raiseTime` (the engine plays both). Typical values: SMG→AR about 0.6 + 0.8 = 1.4 s; pistol→SMG about 0.45 + 0.65 = 1.1 s. `firstRaiseTime` is the longer draw animation played the first time a newly acquired weapon comes out. `quickRaise/quickDrop` are used after offhand actions (grenade throw, knife).
- Sprint-in is 0.3 s for almost everything (Trench Gun 0.16).

---

## 3. Bullet penetration

- BO1 weapon files give each bullet weapon a `penetrateType`. **small**: pistols (Python medium), SMGs, BO1 shotguns. **medium**: ARs, FN FAL, Python, Trench Gun. **large**: M14, snipers, LMGs. No BO1 Zombies weapon sets `fullMetalJacket` or `armorPiercing`. `rifleBullet` is 1 on ARs, snipers, LMGs, M14 and MP40 [WF].
- The type indexes a per-surface **max depth (inches) table**. Official WaW SP table [WAW-WF] `info/bullet_penetration_sp`:

| Surface | small | medium | large |
|---|---|---|---|
| **flesh** | **32** | **96** | **128** |
| wood | 72 | 128 | 192 |
| plaster | 16 | 48 | 128 |
| brick | 0 | 24 | 64 |
| metal | 4 | 8 | 16 |
| concrete | 0 | 8 | 24 |
| glass | 192 | 192 | 192 |

  (The MP table is much stingier: flesh 24/34/40.)
- How this plays in Zombies: a bullet keeps travelling through bodies while the accumulated flesh depth stays under the type's budget. A zombie body is roughly 10–16 in thick, so small (32) passes through about 2 zombies, medium (96) through about 6, and large (128) through about 8. Total surfaces are also capped by the engine. **The damage kept after each penetration is engine code and is not in any script or rawfile.** See Not verified.
- No script in Zombies touches penetration. `_zombiemode.gsc` sets no penetration dvars.

---

## 4. Explosives and Player self-damage

### Self-damage rule [BO1-GSC] `_zombiemode.gsc player_damage_override`
```
if MOD_PROJECTILE / MOD_PROJECTILE_SPLASH / MOD_GRENADE / MOD_GRENADE_SPLASH:
    if HasPerk(PhD Flopper): return 0
    if self.health > 75:      return 75
(else full damage)
```
- **Every self-inflicted explosive hit costs exactly 75 HP while you have more than 75 HP, whatever the weapon.** At 75 HP or less the full engine damage applies, which in practice downs you.
- Without Jugg (100 HP): one blast leaves you at 25. A second blast before regen downs you. With Jugg (250 HP): 75 per hit, so you down on the 4th hit (250 → 175 → 100 → 25 → down).
- **Placeable mines (Claymore) never damage players** (`is_placeable_mine(sWeapon)` returns 0).
- Any damage taken while in last stand is ignored.
- WaW [WAW-GSC]: `player_damage_override` has **no** explosive cap, so your own explosives deal full engine damage. PhD Flopper doesn't exist yet.

### Explosive weapon numbers

| Weapon | Impact | Splash inner–outer | Radius (in) | Source / notes |
|---|---|---|---|---|
| Ray Gun | 1000 | 1500–300 | 64 | [WAW-WF] official `ray_gun` = [WIKI]. [WF] `ray_gun_zm` says 2500 / 2500–1250, which is **mod-tuned** |
| M72 LAW | 2000 | [WIKI] **320–100**; [WF] 1500–1000 | 225 [WF] | conflict, unresolved. Projectile speed 2500 |
| China Lake | 600 | 600–75 | 200 | [WF] = [WIKI]. Projectile speed 2400, arcing grenade |
| Mustang & Sally (PaP M1911, solo last-stand pistol) | [WIKI] 1000 | [WIKI] 1200; [WF] 2500–1000 | 200 | from #3 |
| Crossbow bolt | 750 (sticks) | 400–75 | 200 | [WF] `explosive_bolt_zm` = [WIKI]. 1.5 s fuse |
| Awful Lawton bolt (PaP) | 750 | 2500–225 | 200 | [WF] = [WIKI]. 4 s fuse |
| Frag grenade | – | [WF] 300–75 (WaW stick grenade 200–50) | 256 | §5 |
| Claymore | – | 200–50 | 200 | §5 |
| Monkey Bomb | – | 5000–450 | 100 | §5 |

Player self-hits from any of these cost 75 (rule above). [WIKI] also notes the crossbow splash "stuns" (shellshock) without Flopper.

---

## 5. Equipment

### Slots [BO1-GSC]

| Slot | Kino item | How it is set |
|---|---|---|
| Lethal grenade | Frag (`frag_grenade_zm`) | `get/set_player_lethal_grenade` |
| Tactical grenade | Monkey Bomb | `set_player_tactical_grenade("zombie_cymbal_monkey")` |
| Placeable mine | Claymore | action slot 4 (`setactionslot(4,"weapon","claymore_zm")`) |
| Melee | default knife → Bowie | `set_player_melee_weapon("bowie_knife_zm")`, takes `knife_zm` |
| Primary (2 slots on Kino) | Crossbow, Ballistic Knife, Thundergun, Ray Gun and the rest | these use a gun slot |

**Max Ammo** [BO1-GSC] `full_ammo_powerup`: `GiveMaxAmmo` on **every** weapon in `GetWeaponsList()`. That refills frags to 4, Claymores to 2, Monkeys to 3, Ballistic Knife reserve, and every gun.

### Frag grenade
- **Per-Round refill** [BO1-GSC] `award_grenades_for_survivors`, run at the start of every round including round 1. Holding 0 gives 2, holding 1 gives 3, holding 2 or more gives 4. So you get **+2 per round, max 4**, and start round 1 with 2. `round_start` first gives the weapon with 0. Identical in WaW with `stielhandgranate` [WAW-GSC].
- Wall buy 250 (from #3). It gives start ammo (4).
- Fuse **3.5 s**, cookable (`cookOffHold 1`). Radius 256. Damage [WF] 300–75 (BO1, unverified) and [WAW-WF] 200–50 (WaW, official).

### Claymore [BO1-GSC] `_zombiemode_claymore.gsc`
- **1000, a one-time buy.** After buying, the trigger becomes invisible to you and says "already purchased". It gives **2**.
- **Refilled to 2 on every `between_round_over`.** You can also pick placed ones back up.
- **Trigger:** a `trigger_radius` of r=**96**, height 192 (centred so it spans ±96 vertically). It fires only on a zombie that is **in front**, within a 70° half-angle cone (`dot > cos(70)`), farther than 20 in from the mine, with line of sight (`damageConeTrace`). It beeps, then detonates **0.4 s** later.
- Damage 200–50, r200 [WF]. This equals the official WaW Bouncing Betty (200–50, r200) [WAW-WF].
- It never damages players. Shooting it does not set it off unless the shooter is an enemy-team player. The script deletes the oldest claymore beyond 15 only when the `player_sustainAmmo` cheat dvar is set, so normally there is no cap.

### Monkey Bomb [BO1-GSC] `_zombiemode_weap_cymbal_monkey.gsc`
- **Box only** on Kino. **3 carried** (start = max = 3 [WF] = [WIKI]). **No per-Round refill**: only Max Ammo refills it. Uses the tactical slot.
- On landing (velocity 0) the fuse is **reset** (`resetmissiledetonationtime`), so the **8 s** fuse [WF] = [WAW-WF] counts from landing. [WIKI] says the attraction lasts "around seven seconds".
- **Attraction:** `create_zombie_point_of_interest(1536, 96, 10000)`. Every zombie within **1536 in** retargets to it, up to **96 attractors**, and the POI value 10000 outranks players. Zombies get 4 ring positions with 45 in spacing around it. It only works if the landing spot is in an active or playable zone. Zombies within 500 in groan.
- **Explosion** 5000–450 at r100 (kills anything near it). If you're in last stand when thrown, it is deleted.

### Bowie Knife / default knife
- Default `knife_zm`: melee **150**. Bowie (`bowie_knife_zm`, 3000 wall buy, can't rebuy): melee **1000** [WF] = [WIKI].
- Both have a 0.5 s melee time, a lunge range of 128 in, and a 1.0 s charge (lunge) time. The Bowie doesn't change swing speed.
- Buying it plays a flourish animation, then replaces `knife_zm`. It also converts a held Ballistic Knife into its Bowie variant.
- Zombie HP is 150 + 100/round up to round 9 (950), then ×1.1 per round (#3/[T5-CSV]). So the knife one-hits only on round 1, and the **Bowie one-hits on rounds 1–9** (round 10 = 1045 HP).
- WaW (Der Riese): the Bowie is the engine perk `specialty_altmelee` [WAW-GSC]. [WIKI] gives 1150 damage.

### Ballistic Knife (box, limit 1)
- Primary slot. 1 in mag + 4 reserve [WF] = [WIKI]. Reload 1.5 s.
- **Projectile 500** flat (MOD_IMPACT, which never counts as a headshot), speed 2180, **sticks** in zombies and world.
- It can be **retrieved**: a use trigger is linked to the knife or the corpse, and the knife glows after 2 s [BO1-GSC `_ballistic_knife.gsc`].
- **Melee while held: 500**, and **1000 with the Bowie** (`knife_ballistic_bowie_zm`).
- PaP (Krauss Refibrillator): 1000 projectile/melee (Bowie 1500), 1+9. [WIKI]: a blade hit **revives a downed teammate**.
- [WIKI]: the knife is pulled out in last stand instead of the pistol.

### Crossbow (box, limit 1) [WF] = [WIKI], [BO1-GSC] `_zombiemode_weap_crossbow.gsc`
- 1+12, reload 2.75 s. Impact **750**. The bolt (`explosive_bolt_zm`) **sticks to anything**, then explodes after **1.5 s** for 400–75 at r200.
- **PaP Awful Lawton:** blast 2500–225 at r200 after a **4 s** fuse. The bolt is a **monkey-like POI** for its fuse: `create_zombie_point_of_interest(1536, 96, 10000)`, with the same numbers as the Monkey.
  - Stuck in a zombie: that zombie plays taunt animations clawing at it.
  - Stuck in a player (`level.monkey_bolt_holder`): zombies converge on that player.

---

## 6. Thundergun [BO1-GSC] `_zombiemode_weap_thundergun.gsc`

- Box only, limit 1. **Mag 2, reserve 12** (PaP ZeusCannon 4 + 24). Fire interval 0.95 s, reload 2.0–2.1 s. The weapon itself does 0 damage. Everything is scripted on `weapon_fired`.
- **Hit volume:** zombies within **1200 in** of the muzzle, in the front hemisphere (dot ≥ 0), within a **cylinder of radius 180** around the aim line, and with line of sight (`DamageConeTrace`).
- **Effect by distance from the muzzle:**

| Range | Effect |
|---|---|
| < **480** | **Killed** (`DoDamage(health + 666)`) and ragdoll-flung along the view (plus outward). Fling strength 100–200, stronger when closer |
| 480–900 | Knocked down, gibbed (guts/arm), takes 15 damage twice (30 total) |
| 900–1200 | Knocked down, takes 15 damage twice (30 total) |

- Fling-kill points: the 1st zombie gives normal kill points, the 2nd gives 30, the rest give 10 each.
- It also fires a physics cylinder (r600, height 240) that pushes props.
- **The PaP version uses the same range variables.** No upgraded zombie_vars exist. This contradicts [WIKI]'s claim of "more range and bigger radius". The only script difference is ammo.

---

## 7. WaW vs BO1 differences found

| Item | WaW | BO1 |
|---|---|---|
| Own-explosive damage | full engine damage | capped at 75 if HP > 75; 0 with PhD |
| Grenade refill | +2/round max 4 (`stielhandgranate`, 200–50) | same rule (`frag_grenade_zm`) |
| Ray Gun | 1000 + 1500–300 r64 (official file) | same per [WIKI] |
| Bowie | `specialty_altmelee`, [WIKI] 1150 | separate melee weapon, 1000 |
| Mine | Bouncing Betty 200–50 r200 | Claymore 200–50 r200, front cone |
| Monkey | 3 in WaW Der Riese file (CoD:Zombies iOS: 2) | 3, 8 s, 1536 radius |
| Penetration table | flesh 32/96/128 (official) | assumed same (no BO1 table found) |

---

## Not verified
- **Damage kept after bullet penetration**, and the max number of surfaces. Both are engine code. The BO1 `bullet_penetration_sp` table was not found (only WaW's). The "about 2/6/8 zombies" figure is my own estimate from depth ÷ assumed body thickness.
- **Spread units**: I read them as degrees from engine knowledge, not from a source. The handling values (§2) come only from [WF], with no wiki cross-check. The mod is known to have altered some first-commit files.
- **Conflicts:** Spectre, G11 and FAMAS reserves ([WIKI] used). M72 LAW splash (320–100 [WIKI] vs 1500–1000 [WF]). BO1 frag damage (300–75 [WF] only). Mustang & Sally splash.
- **L96A1 500 and FN FAL 160–130** are [WF] only.
- `cz75lh_zm` (the left-hand gun) shows 45–25 damage and head x1.4 in [WF], which looks like a non-Zombies copy. I assumed both guns do 150–100.
- The **Ballistic Knife PaP revive** and the knife appearing in last stand are [WIKI] only. I didn't find the revive in the GSC, so it's probably engine-side.
- **Switch time = drop + raise** is engine behaviour, not read from code.
- The WaW Bowie 1150 figure is [WIKI] only.
