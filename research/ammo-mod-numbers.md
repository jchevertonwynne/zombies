# Ammo Mod numbers (BO3)

Research for [#28](https://github.com/jchevertonwynne/zombies/issues/28). What do BO3 Zombies' five Alternate Ammo Types (our **Ammo Mods**) actually do, in numbers?

## Sources

Primary, in trust order:

1. **Treyarch's BO3 mod-tools script source** (uncompiled, with the `#define` tuning headers), mirrored at [zeroy99/bo3_modtools](https://github.com/zeroy99/bo3_modtools):
   - `scripts/shared/aat_shared.gsc`: the shared trigger roll, cooldowns, one-per-Weapon storage. [link](https://github.com/zeroy99/bo3_modtools/blob/master/scripts/shared/aat_shared.gsc)
   - `scripts/zm/aats/_zm_aat_{blast_furnace,dead_wire,fire_works,thunder_wall,turned}.gsc` + `.gsh`: each Ammo Mod's effect and its tuning numbers. [folder](https://github.com/zeroy99/bo3_modtools/tree/master/scripts/zm/aats)
   - `scripts/zm/_zm_lightning_chain.gsc`: the chain-lightning code Dead Wire calls.
   - `scripts/shared/ai/zombie.gsc`, `scripts/zm/_zm_behavior.gsc`: how a Turned Zombie picks and kills targets.
   - `scripts/zm/_zm_spawner.gsc`, `scripts/zm/_zm.gsc`, `scripts/zm/_zm_score.gsc`: kill credit, Points and Power-up drops.
   - `scripts/zm/_zm_pack_a_punch.gsc`, `scripts/zm/_zm_weapons.gsc`: when an Ammo Mod is given, and the price.
   - `scripts/zm/_zm_ai_{dogs,wasp,raps}.gsc`: immunity registrations.
   - `scripts/shared/util_shared.gsc`: `wait_network_frame` (0.1 s in solo).
2. **Decompiled shipped retail scripts**, [shiversoftdev/t7-source](https://github.com/shiversoftdev/t7-source/tree/main/scripts/zm/aats). Every `aat::register(...)` call and the key constants (Dead Wire chain `(8, 9, 120)`, Fireworks `360000` range and 10 shots, Thunder Wall 360 search / 6 cap, Turned 90 radius / 3 cap / 12 kills / 0.5 s) **match the mod-tools source**, so the mod-tools numbers are what shipped.
3. **callofduty.fandom.com**: wikitext of [Alternate Ammo Type](https://callofduty.fandom.com/wiki/Alternate_Ammo_Type), [Napalm Burst](https://callofduty.fandom.com/wiki/Napalm_Burst) (Blast Furnace), [Dead Wire](https://callofduty.fandom.com/wiki/Dead_Wire), [Fire Works](https://callofduty.fandom.com/wiki/Fire_Works), [Thunder Wall](https://callofduty.fandom.com/wiki/Thunder_Wall), [Brain Rot](https://callofduty.fandom.com/wiki/Brain_Rot) (Turned). Used only as a cross-check of in-play behaviour. Where it disagrees with the script, the script wins, and the disagreement is noted below.

Units: BO3 distances are game units (1 unit = 1 inch; the Fireworks header itself says "600 * 600, 50 feet"). Times are seconds.

## How triggering works (all five)

From `aat_shared.gsc` `aat_response()`, run on **every damage event** a Player deals to a Zombie:

- **Which damage counts:** only `MOD_PISTOL_BULLET`, `MOD_RIFLE_BULLET`, `MOD_GRENADE`, `MOD_PROJECTILE`, `MOD_EXPLOSIVE`, `MOD_IMPACT`. Melee, burn and Ammo-Mod damage itself (`MOD_UNKNOWN`) never roll. So an Ammo Mod's own kills can't chain into another trigger.
- **Order of checks:** has an Ammo Mod → (if this hit kills, does the mod allow on-death triggers?) → archetype not trigger-immune → per-Zombie cooldown → per-Player cooldown → global cooldown → the mod's own validation → roll `percentage >= RandomFloat(1)`.
- **Chance is flat.** One number per Ammo Mod. There is **no per-weapon-class scaling** anywhere in the code. A shotgun or other multi-pellet Weapon gets one roll per damage event, so more pellets landing gives more rolls. That last part is inferred from the per-damage-event hook; it has not been tested in game.
- **Cooldowns start only on a successful trigger**, and they all start together (per-Player and global). The per-Zombie cooldown is 0 for all five, so it never matters.
- **Reroll:** the Lucky Crit GobbleGum (`register_reroll`) adds extra rolls at the same chance. Not relevant to us.
- **One Ammo Mod per Weapon in BO3.** `self.aat[weapon] = name` stores a single name per Weapon. A Repack picks uniformly at random from the other four (the current one is removed from the pool). **So two Ammo Mods can never trigger on the same hit in BO3: a Weapon can't hold two.** Our two-mod Weapon has no BO3 precedent.
- **Getting one:** only a Repack of an already-Upgraded Weapon gives one (`weapon_supports_aat` requires an upgraded root weapon). Repack costs 2500 Points (500 during Bonfire Sale); first Pack-a-Punch costs 5000 (1000).

## The numbers

Columns `chance`, `Player cd` and `global cd` come straight from the `aat::register` call. "Global" is shared by every Player and every Weapon with that mod. Solo play is the only case that matters for us, so in practice the binding cooldown is max(Player, global).

| Ammo Mod | Chance per hit | Player cooldown | Global cooldown | Triggers on killing hit? | Radius | Zombie cap | Kill speed |
|---|---|---|---|---|---|---|---|
| Blast Furnace | 15% | 15 s | 0 | yes | 120 | none | shot Zombie instant; others burn to death in 2.75 s |
| Dead Wire | 20% | 5 s | 2 s | yes | 120 → shrinking per jump | 9 Zombies | instant per jump, staggered 0.2–0.6 s × jump depth |
| Fireworks | 10% | 20 s | 10 s | yes | 600 targeting | none (10 shots) | shot Zombie instant; each phantom hit is an instant kill |
| Thunder Wall | 25% | 10 s | 0 | yes | 180, front half only | 6 Zombies (incl. shot one) | instant |
| Turned | 15% | 15 s | 8 s | **no**, needs a non-lethal hit | 90 blast on turning | 3 blasted; 12 melee kills | Turned ally lasts 20 s or 12 kills |

### Blast Furnace (`_zm_aat_blast_furnace`)

- 15% chance, 15 s Player cooldown, no global cooldown, can trigger on the killing hit.
- On trigger: the shot Zombie is gibbed and killed outright (`DoDamage(self.health)`, credited to the Player).
- Every other enemy within **120 units** of the shot Zombie is set alight. **No cap** on how many.
- After a **0.25 s** delay, each burning Zombie takes `health / 6` per tick, ticking every **0.5 s**. The tick damage is fixed from its health when the burn started. It dies on the 6th tick, **2.75 s after the trigger**. The loop allows a 7th tick, but it never fires on a Zombie that is already dead. Burning Zombies keep attacking until they die (wiki agrees, "3–5 seconds").
- Damage is a share of current health, so it **kills at any Round**.

### Dead Wire (`_zm_aat_dead_wire` + `_zm_lightning_chain`)

- 20% chance, 5 s Player cooldown, 2 s global cooldown, can trigger on the killing hit.
- Chain params `create_lightning_chain_params(max_arcs = 8, max_enemies_killed = 9, radius_start = 120)`. Defaults kept: `radius_decay = 20` per jump, head gib 100% (overridden), network death choke 4 per 0.2 s (solo).
- Jump radius at depth *n* is `120 − 20n`: 100, 80, 60, 40, 20, then 0, which stops the chain. **In practice the radius ends the chain after 5 jumps, before the 8-jump cap is reached.**
- Each jump needs a clear line (`BulletTracePassed`) from the source Zombie's head to the target's head. A Zombie is never hit twice in one chain.
- **Hard cap: 9 Zombies per trigger**, counting the shot one.
- Each hit Zombie takes `health + 666` damage (**kills at any Round**) after a delay of `rand(0.2..0.6) × depth` for depth > 1.
- Credited to the Player, with the Pack-a-Punched Weapon as the kill weapon. Pays a normal kill reward (the `"death"` event). The code's "Power-up after 10 chain kills" (`kills_for_powerup = 10`) is **commented out**, so there is no bonus drop. Normal per-kill drops apply; see Points below.
- The wiki claims Zombies past the 9 cap are "stunned". **Not in the script.** Zombies past the cap are just un-flagged and take no damage. The `ZM_AAT_DEAD_WIRE_ZOMBIE_SLOW_TOTAL_TIME 1` define is never used.

### Fireworks (`_zm_aat_fire_works`)

- 10% chance, 20 s Player cooldown, 10 s global cooldown, can trigger on the killing hit.
- Won't trigger on a Zombie that is entering through a barricade, traversing, leaping, or still emerging.
- On trigger: the shot Zombie is gibbed and killed (credited to the Player).
- **Phantom gun:** a floating model of the Player's **currently held Weapon** (read at trigger time) spawns at the dead Zombie's feet. It rises **56 units** over **0.5 s**.
- It then fires **10 shots, one per network frame** (0.1 s in solo, so about **1 s** of firing). It returns down over 0.5 s and is deleted.
- **Aim, re-picked for every shot:** shuffle all enemies, then take the first within **600 units** (50 ft). Line of sight to the gun is checked for at most **3** candidates; after 3 failures, line of sight is no longer required. If none are in range, it falls back to the first shuffled enemy **anywhere on the map** (a quirk). If there are no enemies at all, it fires at a random horizontal heading.
- Each shot is a real `MagicBullet` of that Weapon aimed at the target's centre. **Any Zombie its bullets damage is killed outright** (gib + `DoDamage(health)`), so it kills at any Round. It can kill fewer than 10 if shots miss, or more if a bullet penetrates or splashes.
- Phantom bullets can't hurt Players (`_zm.gsc` returns 0 damage for `b_aat_fire_works_weapon`).
- **The phantom gun's kills are credited to the gun model, not the Player.** Kill Points and Power-up drops both require a Player attacker, so **they pay no Points, drop no Power-ups and don't count as Player kills**. Only the shot Zombie does. The wiki agrees ("any zombies killed afterward will not generate any points"). The death callback that fetches `attacker.owner` is an empty stub.

### Thunder Wall (`_zm_aat_thunder_wall`)

- 25% chance, 10 s Player cooldown, no global cooldown, can trigger on the killing hit. The wiki says "approximately 15 seconds"; **the script says 10.**
- On trigger, centred on the shot Zombie:
  - Candidates are enemies within 360 units, closest first.
  - The shot Zombie is always hit.
  - Every other candidate must be in the **front half-space** of the Player's weapon-forward direction (dot ≥ 0) **and within 180 units** of the blast point. A segment check (within 540 of a 180-long line) also runs, but the 180 limit is the one that binds.
- Each one hit takes `DoDamage(health)` with `MOD_IMPACT`, credited to the Player (**instant kill at any Round**). It is ragdoll-launched at force **100**, pointing away from the blast plus **30** units of upward bias and ±3 random sideways.
- **Cap: 6 Zombies**, including the shot one.
- Pays normal kill Points. The wiki notes that an early patch fixed a bug where it gave no Points or kills; the current script credits the Player.

### Turned (`_zm_aat_turned`)

- 15% chance, 15 s Player cooldown, 8 s global cooldown. **Does not trigger on a killing hit** (`occurs_on_death = false`). Won't trigger on a Zombie that is immune, entering through a barricade, traversing, still emerging, or leaping.
- On trigger:
  - The shot Zombie switches to the Player's team (`team = "allies"`).
  - It can't die or flinch (`allowDeath = false`, `allowpain = false`) and won't gib.
  - It is forced to sprint.
- **Turn blast:** up to **3** other enemies within **90 units** are killed (`DoDamage(health)`, `MOD_IMPACT`, credited to the Player, so they pay Points) and ragdoll-launched at force **60**, with **10** upward bias.
- **Targeting** (`_zm_behavior.gsc` `findZombieEnemy`): every think, it picks the **closest living enemy** that has finished emerging. That must be a basic Zombie or an enemy flagged `canBeTargetedByTurnedZombies`. It then paths to it. It ignores barricades and attackables.
- **Kill:** on its melee notetrack, a basic Zombie is gibbed and killed (`Kill()`), as are a Nova crawler or spider. That is **one swipe = one kill, at any Round**. Other flagged enemies just take a normal melee.
- **Lifetime:** ends at **20 s** or **12 kills** (+0.5 s for the swing to finish), whichever comes first. Then it gibs and dies (`DoDamage(health)` with **no attacker**, so no Points).
- **The Turned Zombie's own kills use `Kill()` with no attacker. They pay no Points, drop no Power-ups and aren't credited to the Player.** Only the 3-Zombie turn blast pays.
- The wiki says Turned Zombies "are ignored by round progression". That fits their being on the allies team, but the Round-counting code was not checked. **Unverified.**

## Immunities and high Rounds

- **All five kill a basic Zombie outright at any Round.** Damage is always "current health" (or health + 666, or health/6 × 6), never a fixed number. Nothing in these scripts scales with Round.
- Immunity is per archetype, as three flags: can it trigger the mod, is it immune to the direct effect, is it immune to the splash. Set in the base scripts:
  - **Hellhounds:** Fireworks and Turned can't trigger on them. Blast Furnace and Dead Wire can trigger but do nothing to them. Thunder Wall kills them but doesn't fling them (the "indirect" flag only gates the ragdoll launch).
  - **Parasites (wasps):** immune to all five.
  - **Meatballs / RAPS:** Dead Wire, Fireworks and Turned can't trigger. Blast Furnace triggers and burns them (the shot one is burned, not instantly killed). Thunder Wall kills them but doesn't fling them.
- Bosses and special enemies (Margwa, Panzer, etc.) register their own immunities in map scripts, which were **not surveyed**. The `register_immunity` doc example shows the Margwa immune to Blast Furnace's effects.

## Points, Power-ups, kill credit

From `_zm_spawner.gsc` `zombie_death_points()`: any death whose attacker is a **Player** pays the normal kill Points and rolls the normal Power-up drop. It also counts as a Player kill (`zombie_death_event`).

| Ammo Mod | Kill credited to Player → Points + Power-up roll + kill stat |
|---|---|
| Blast Furnace | yes: shot Zombie and every burn death |
| Dead Wire | yes: every Zombie in the chain |
| Fireworks | shot Zombie only; **phantom gun kills: no** |
| Thunder Wall | yes, all ≤ 6 |
| Turned | 3-Zombie turn blast: yes; **Turned Zombie's own kills: no**; its own death: no |

None of them drops a *guaranteed* Power-up. The Dead Wire 10-kill drop is commented out.

## Unverified / gaps

- **Multi-pellet Weapons rolling once per pellet.** Inferred from the per-damage-event hook; not tested in game.
- **Fireworks firing time.** "About 1 s" assumes solo `wait_network_frame` = 0.1 s, which is the code path when `NumRemoteClients() == 0`. Not measured.
- **Turned Zombies and the Round's remaining-Zombie count.** Not checked; claimed only by the wiki.
- **Dead Wire "stunning" Zombies past the 9 cap.** Wiki only; absent from the script.
- **Boss and special-enemy immunities.** Set per map, not surveyed.
- **Whether a kill Point value differs** for `MOD_UNKNOWN` / `MOD_IMPACT` kills versus normal bullet kills (headshot bonus, etc.). `_zm_score.gsc` gives no headshot bonus without a head hit location. Ammo-Mod kills presumably pay the base kill reward (the wiki says 50 for Dead Wire and Thunder Wall), but this was not traced line by line.
