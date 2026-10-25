# Roguelite design references for Gauntlet

Answers [#20](https://github.com/jchevertonwynne/zombies/issues/20), a child of the map [#1](https://github.com/jchevertonwynne/zombies/issues/1).

**The question:** how do established roguelites and roguelite-flavoured shooters structure their in-run upgrade offers and their meta-progression? What carries over to **Gauntlet**, where a **Boon** is offered after every **Round** of a CoD-style zombies **Run**? (Terms are as defined in `GLOSSARY.md`.)

The references are Enter the Gungeon, Vampire Survivors, Hades, Brotato, Risk of Rain 2, and Treyarch Zombies (BO3/BO6 GobbleGums, BO4 Elixirs, Cold War Aetherium upgrades, BO6 Augments).

## Sources and trust

None of these studios publishes a design spec, so the best primary sources are the community wikis. Their numbers are mostly datamined from the game files, and patch history is quoted where it matters. Developer statements are cited where I could find them, and they were thin: no studio has a GDC talk on its upgrade-offer design. If a claim rests on my own knowledge of the game rather than on a fetched page, it says so.

| Tag | Source |
|---|---|
| **[H-Boons]** | [Hades Wiki: Boons](https://hades.fandom.com/wiki/Boons) |
| **[H-Pom]** | [Hades Wiki: Pom of Power](https://hades.fandom.com/wiki/Pom_of_Power) |
| **[H-Mirror]** | [Hades Wiki: Mirror of Night](https://hades.fandom.com/wiki/Mirror_of_Night) |
| **[H-God]** | [Hades Wiki: God Mode](https://hades.fandom.com/wiki/God_Mode), citing [Supergiant's announcement](https://twitter.com/SupergiantGames/status/1309542802196324353) |
| **[H-Pact]** | [Hades Wiki: Pact of Punishment](https://hades.fandom.com/wiki/Pact_of_Punishment) |
| **[VS-Lvl]** | [Vampire Survivors Wiki: Level up](https://vampire.survivors.wiki/w/Level_up) |
| **[VS-PU]** | [Vampire Survivors Wiki: PowerUps](https://vampire-survivors.fandom.com/wiki/PowerUps) |
| **[VS-Evo]** | [Vampire Survivors Wiki: Evolution](https://vampire-survivors.fandom.com/wiki/Evolution), [Treasure Chest](https://vampire-survivors.fandom.com/wiki/Treasure_Chest) |
| **[VS-Arc]** | [Vampire Survivors Wiki: Arcanas](https://vampire-survivors.fandom.com/wiki/Arcanas) |
| **[VS-Reaper]** | [Vampire Survivors Wiki: The Reaper](https://vampire-survivors.fandom.com/wiki/The_Reaper) |
| **[B-Shop]** | [Brotato Wiki: Shop](https://brotato.wiki.spellsandguns.com/Shop) |
| **[B-Upg]** | [Brotato Wiki: Upgrades](https://brotato.wiki.spellsandguns.com/Upgrades) |
| **[B-Waves]** | [Brotato Wiki: Waves](https://brotato.wiki.spellsandguns.com/Waves) |
| **[B-Stats]** | [Brotato Wiki: Stats](https://brotato.wiki.spellsandguns.com/Stats) |
| **[R-Stack]** | [Risk of Rain 2 Wiki: Item Stacking](https://riskofrain2.wiki.gg/wiki/Item_Stacking) |
| **[R-Chest]** | [Risk of Rain 2 Wiki: Chests](https://riskofrain2.wiki.gg/wiki/Chests) |
| **[R-Diff]** | [Risk of Rain 2 Wiki: Difficulty](https://riskofrain2.wiki.gg/wiki/Difficulty) |
| **[R-Art]** | [Risk of Rain 2 Wiki: Artifacts](https://riskofrain2.wiki.gg/wiki/Artifacts) |
| **[R-Print]** | [Risk of Rain 2 Wiki: 3D Printers](https://riskofrain2.wiki.gg/wiki/3D_Printers) |
| **[R-Lunar]** | [Risk of Rain 2 Wiki: Lunar Coin](https://riskofrain2.wiki.gg/wiki/Lunar_Coin) |
| **[G-Chest]** | [Enter the Gungeon Wiki: Chests](https://enterthegungeon.fandom.com/wiki/Chests) |
| **[G-Mag]** | [Enter the Gungeon Wiki: Magnificence](https://enterthegungeon.fandom.com/wiki/Magnificence) |
| **[G-Syn]** | [Enter the Gungeon Wiki: Synergies](https://enterthegungeon.fandom.com/wiki/Synergies) |
| **[G-Curse]** | [Enter the Gungeon Wiki: Curse](https://enterthegungeon.fandom.com/wiki/Curse), [Coolness](https://enterthegungeon.fandom.com/wiki/Coolness) |
| **[G-Meta]** | [Enter the Gungeon Wiki: Pickups § Hegemony Credits](https://enterthegungeon.fandom.com/wiki/Pickups), [Unlocks](https://enterthegungeon.fandom.com/wiki/Unlocks) |
| **[Z-GG3]** | [CoD Wiki: GobbleGum/Black Ops III](https://callofduty.fandom.com/wiki/GobbleGum/Black_Ops_III) |
| **[Z-GG6]** | [CoD Wiki: GobbleGum/Black Ops 6](https://callofduty.fandom.com/wiki/GobbleGum/Black_Ops_6), [GobbleGum](https://callofduty.fandom.com/wiki/GobbleGum) |
| **[Z-Elix]** | [CoD Wiki: Elixir](https://callofduty.fandom.com/wiki/Elixir) (BO4) |
| **[Z-Aeth]** | [CoD Wiki: Aetherium Crystals](https://callofduty.fandom.com/wiki/Aetherium_Crystals), citing the [Activision Season Two blog](https://www.callofduty.com/blog/2021/02/Everything-You-Need-to-Know-About-Call-of-Duty-Black-Ops-Cold-War-and-Warzone-Season-Two-Coming-February-25) |
| **[Z-Aug]** | [CoD Wiki: Augments](https://callofduty.fandom.com/wiki/Augments) (BO6/BO7) |

---

## 1. Offer shape

| Game | Choices per offer | Rarity tiers | Reroll / banish / skip | Pity or bias rules |
|---|---|---|---|---|
| **Hades** | Usually 3 from one god (my knowledge; the fetched page doesn't state it) | Common, Rare, Epic, Heroic. Duo and Legendary sit outside the ladder [H-Boons] | Fated Persuasion: up to 4 dice per Run to reroll a boon offer. Fated Authority: up to 8 dice to reroll what the next room gives [H-Mirror] | A Keepsake forces the next god offered. Duos and Legendaries need specific boons first. Harder rooms give higher rarity [H-Boons] |
| **Vampire Survivors** | 3, sometimes 4 (the chance of a 4th scales with Luck) [VS-Lvl] | None on level-up. Items have levels instead | Reroll, Skip and Banish, each bought permanently at +2 uses per rank, up to 10 per Run. Seal banishes items before the Run [VS-PU] | Full slots (6 weapons, 6 passives) stop new items appearing. Maxed items stop being offered. With nothing left to offer, it gives gold or food [VS-Lvl] |
| **Brotato** | Level-up: 4 stat upgrades [B-Upg]. Shop: up to 4 items [B-Shop] | Tiers I–IV. Odds climb by wave and are multiplied by Luck, then capped [B-Shop] | Paid reroll whose cost rises within a shop and resets each wave. Free, unlimited lock carries an item to the next shop [B-Shop] | Waves 1–2 guarantee 2 weapons and 2 items; waves 3–5 guarantee 1 weapon [B-Shop] |
| **Risk of Rain 2** | 1 by default: a chest gives a random item. The Artifact of Command turns each drop into a pick from every item of that rarity [R-Art] | White, Green, Red, plus Boss, Lunar and Void. A small chest is 79.2 / 19.8 / 0.99% [R-Chest] | No reroll. 3D Printers trade a random item for a chosen item of the same rarity, and Scrappers turn items into scrap [R-Print] | None found |
| **Enter the Gungeon** | 1: a chest holds one item or gun, with no choice | D, C, B, A, S chests [G-Chest] | None. You can shoot a chest for a worse drop [G-Chest] | **Synergy Factor:** synergy-completing drops start 1.8× likelier (6× on a fresh save) and fall to about 1× after 2–3 synergies [G-Syn]. **Magnificence:** each A or S pickup raises the chance that the next A or S drops a tier [G-Mag]. Room-clear rewards get likelier the longer you go without one [G-Curse] |
| **BO3 GobbleGum** | 1 random from a 5-gum pack the player builds before the match [Z-GG3] | Classic, Mega, Rare Mega, Ultra-rare Mega [Z-GG3] | Buy again (up to 3 times a Round, at rising Points prices) [Z-GG3] | None. The pack *is* the bias |
| **BO6 GobbleGum** | Free from a machine, hold up to 3, use any time [Z-GG6] | Rare, Epic, Legendary, Ultra, Whimsical [Z-GG6] | None | Loadout pack. Using a gum spends it from a meta-stock; receiving it does not [Z-GG6] |

**The patterns:**

- **There are two styles of choice.** In the *pick-one-of-N* games (Hades, VS, Brotato), the choice is the game. Gungeon, RoR2's default rules and BO3 GobbleGum give one random item, so the drama comes from luck and the skill is in adapting. RoR2 ships the choice style as an optional modifier (Command), so the developer judged random drops to be the default experience.
- **Offers of 3–4 are the norm.** VS and Brotato make a 4th slot (or more) a stat to chase (Luck).
- **Controls over the offer are rationed, not free.** Hades gives at most 4 Persuasion dice per Run. VS gives at most 10 rerolls, 10 skips and 10 banishes, each unlocked through meta-progression. Brotato's rerolls cost more each time. In every case, rerolls are a resource you earn or buy.
- **Hidden bias rules are common and invisible to players.** Gungeon nudges you toward a synergy early and away from stacking A or S items. Hades raises rarity in harder rooms. Brotato front-loads weapons. These make runs feel deliberate without the player seeing a rule.

## 2. Pick cadence, run length, and keeping power in check

| Game | Run length | Pick cadence | Power-growth brakes |
|---|---|---|---|
| **Vampire Survivors** | 30 min, then the Reaper kills you (65,535 damage, one more every minute) [VS-Reaper] | Every level-up. The XP needed per level grows by 10, then 13, then 16, with big bumps at levels 20 and 40 [VS-Lvl] | Hard slot cap (6 + 6), a max level per item, and maxed items leave the pool [VS-Lvl]. Evolutions need a max-level weapon, its passive, and a chest after 10:00 [VS-Evo]. Arcanas are picked only at 0:00, 11:00 and 21:00 [VS-Arc] |
| **Brotato** | 20 waves. Waves run 20 s, growing 5 s per wave to 60 s, and wave 20 is 90 s [B-Waves] | Every wave: level-up upgrades, then a shop [B-Waves] | Tier odds have caps (Tier IV max 8%) [B-Shop]. Stat caps: Dodge 60%, crit 100%, attack speed has diminishing returns and a ceiling of 12 attacks per second [B-Stats]. Elite and Horde waves come at fixed later waves on higher Danger [B-Waves] |
| **Hades** | About 4 regions of chambers | Boon rewards in many but not all chambers. Each room shows its reward in advance, so you choose a path | Each ability (Attack, Special, Cast, Dash, Call) holds **one** boon slot [H-Boons]. Pom of Power level-ups have **diminishing returns**, mostly in the first 2–3 levels, with a floor; Duos and Legendaries can't be levelled [H-Pom]. Heat/Pact makes things harder by opt-in [H-Pact] |
| **Risk of Rain 2** | Open-ended loop | Every chest, bought with gold whose cost scales with difficulty | **Stacking curves:** *linear* (1 + a·x), *hyperbolic* for chances and reductions (1 − 1/(1 + a·x), so 10 Tougher Times = 60% block, never 100%), and rare *exponential* (Shaped Glass ×2 damage per stack, but halves health) [R-Stack]. **Difficulty is driven by time:** coeff = (playerFactor + minutes × 0.0506 × difficulty) × 1.15^stages. Enemy level and chest prices (baseCost × coeff^1.25) both scale off it [R-Diff] |
| **Enter the Gungeon** | 5–6 floors | Chests per floor, plus a boss drop and a chance at a reward after each room | Magnificence (above). Quality odds rise by floor. Curse makes enemies Jammed but drops more ammo, so power can be bought with risk [G-Curse] |
| **Treyarch Zombies** | Open-ended Rounds | BO3: GobbleGum up to 3 times a Round, at a price that resets each Round and rises with the Round number [Z-GG3]. BO6: one free gum per 10 s cooldown, from a stock you own outside the match [Z-GG6] | Gums are mostly **temporary** (Round-based, timed or one-shot) [Z-GG3]. Zombie Health keeps rising; at Round 10 and above it is ×1.1 per Round in BO1 (see `research/core-loop-numbers`) |

**The patterns:**

- **Most give the player more picks per minute early and fewer late** (VS's XP curve, Brotato's waves getting longer). Gauntlet's fixed "one per Round" goes the other way, because CoD Rounds get *longer* as they go, so picks naturally slow down late. That matches the genre norm without extra work.
- **Two kinds of brake.** *Structural* brakes are slot caps, one boon per ability, items leaving the pool, and tier caps. *Mathematical* brakes are diminishing or hyperbolic stacking and hard stat caps. Hades leans structural, RoR2 mathematical, and VS and Brotato use both.
- **The enemy curve does the real work.** RoR2 and CoD Zombies both rely on enemies scaling faster than the player can stack power in the long run. In CoD that is exponential Zombie Health from Round 10. Boons that scale with a percentage (damage %) keep pace with that curve. Flat boons (+N damage) stop mattering. That is a design choice, not an accident.

## 3. Boon categories that work, and ones known to break

**Categories that work well:**

- **Stat bumps** (Brotato upgrades [B-Upg], VS PowerUps [VS-PU], Mirror talents [H-Mirror]). They are easy to read but dull by themselves, and they work as filler or as a floor.
- **Changes to how a verb behaves** (Hades: "your Cast is a wide blast that inflicts Weak", "your Dash creates a Blade Rift" [H-Boons]). This is Hades' core: one slot per verb, and the boon changes *how* the verb works. The zombies version would be Weapon fire, Reload, Knife, sprint, Lethal and Tactical.
- **On-event triggers** (Hades "after you take damage…", "when foes are slain…" [H-Boons]; BO6 Augments "melee kills reload a portion of your held weapon" [Z-Aug]). These suit horde play, where events happen constantly.
- **Economy** (Hades Poseidon "chamber rewards worth more", Hermes "gain wealth each chamber" [H-Boons]; Brotato Harvesting [B-Upg]; RoR2 gold scaling [R-Diff]). In Gauntlet these would interact with Points and every Buyable.
- **Synergies and evolutions** (VS evolutions: max weapon + a passive + a chest [VS-Evo]; Gungeon synergies [G-Syn]; Hades Duos [H-Boons]). These give long-term goals within a Run, and they are where "build identity" comes from.
- **Changes to an existing system** (BO6 Augments change Perks: "Quick Revive: if downed, killing an enemy revives you… up to 3 times" [Z-Aug]; BO4 Elixirs spawn Power-ups, e.g. "Dead of Nuclear Winter: spawns a Nuke" [Z-Elix]). In a zombies game this layer is already full of hooks: Perks, Power-ups, Traps, the Mystery Box, Pack-a-Punch and Barriers.

**Known balance breakers and hazards (as documented):**

- **Exponential stacking.** Shaped Glass doubles damage per stack. Its only check is that it also halves health, and the wiki notes a hard limit of 127 copies because the 128th kills you [R-Stack].
- **Linear chance stacking.** RoR2 moved chance effects to hyperbolic stacking so they can't reach 100% [R-Stack]. A linear "+15% dodge" would reach immunity at 7 stacks.
- **Infinite loops between items.** Gungeon's wiki lists three combinations that give infinite Coolness, one of which also gives infinite ammo [G-Curse]. Any boon that refunds a resource and is triggered by spending that resource needs a guard.
- **Pay-to-win meta stock.** BO3 Mega GobbleGums are consumed from a collection and can be bought outside the game [Z-GG3, Z-GG6]. This doesn't apply to us (no store), but meta-stocked consumables still let a player "save up" power and bring it into a Run.
- **Losing Perks on a Down.** BO4's "Aftertaste" ("keep all perks when being revived") and BO6's EMT augment [Z-Elix, Z-Aug] show that keeping Perks through a Down is a coveted effect. Since Boons survive a Down by definition, Gauntlet already partly does this. A Boon that also keeps Perks would remove Classic's main penalty for going down.

## 4. Does choosing pause the game, and what does the choice UI look like?

| Game | Pauses? | UI |
|---|---|---|
| **Vampire Survivors** | **Yes.** The game pauses on level-up, then gives brief invulnerability after the pick [VS-Lvl] | A modal list of 3–4 cards, with Reroll, Skip and Banish buttons |
| **Brotato** | **Effectively yes.** Upgrades and the shop come between waves, outside combat [B-Waves] | A full-screen shop with 4 cards, reroll, lock, and a stats panel |
| **Hades** | Effectively yes: the boon menu opens after the room is cleared (my knowledge) | 3 large cards showing rarity colour, the god's portrait and voice line, and the numbers |
| **Risk of Rain 2 (Command)** | **No.** "Players are still vulnerable to damage while selecting an item" [R-Art] | A radial or grid picker |
| **BO3 / BO6 GobbleGum** | **No.** The machine is used in real time during play [Z-GG3, Z-GG6] | The machine gives the gum. In BO6 a 3-slot inventory is used from the HUD |

CoD Zombies already has a natural lull: the **Intermission** between Rounds. Gauntlet's offer fits the Brotato or Hades model (the choice comes in a lull) without a hard pause. The open question is what happens when the player is still kiting Zombies left over from the Round, or picks late.

## 5. Meta-progression

| Game | Currency | What it buys | Permanent stats? | Effect on the "fresh start" feel |
|---|---|---|---|---|
| **Hades** | Darkness, Chthonic Keys | Mirror talents (red/green pairs, freely refundable for a Key) [H-Mirror]. Talents include boon-rarity odds (Olympian Favor +40% Rare, Gods' Pride +20% Epic, Gods' Legacy +10% Duo/Legendary) and the reroll dice | **Yes**, large ones (+50 Health, 3 Death Defiances) | Kasavin says the Mirror exists to make you meaningfully stronger from one run to the next ([Tom's Guide interview](https://www.tomsguide.com/uk/features/hades-exclusive-interview-supergiant); I found it through a search snippet and couldn't load the full page). Heat later lets you turn the difficulty back up [H-Pact]. God Mode adds +2% damage resistance per death, capped at 80% [H-God] |
| **Vampire Survivors** | Gold | PowerUps: Might +25%, Max Health +30%, Revival, Amount +1, and the Reroll, Skip and Banish counts [VS-PU] | **Yes**, and each can be switched off once maxed [VS-PU]. A Curse PowerUp makes the game *harder* for more reward | Strong power creep, but the player can switch it off |
| **Brotato** | None found in the sources I read | Characters and items unlock through play (my knowledge) | None found | Each Run starts from the same baseline. Difficulty comes from Danger levels 0–5 [B-Waves] |
| **Risk of Rain 2** | Lunar Coins (kept between runs) | Lunar items, Bazaar access, some unlocks [R-Lunar] | **None** mentioned [R-Lunar] | Unlocks widen the item and character pool. Runs start equal |
| **Enter the Gungeon** | Hegemony Credits (from bosses, doubled for a hitless boss kill) | Items in the Breach that "will show up in future runs", 1,804 credits for everything; shortcuts; modes [G-Meta] | **None.** Unlocks only add to the pool [G-Meta, Unlocks] | Pure fresh start. Meta-progression only adds variety, and buying junk can dilute the pool |
| **Cold War Zombies** | Aetherium Crystals (Raw, Refined, Flawless), earned at Round 10 and every 5 Rounds after [Z-Aeth] | Upgrading Perks, Field Upgrades, Ammo Mods and Weapon Classes to tier 5 [Z-Aeth] | **Yes**, per Perk | Perks behave differently for a veteran and a new player |
| **BO6 Augments** | XP put into research | 3 Major and 3 Minor augments per Perk, Ammo Mod or Field Upgrade; equip 1 of each [Z-Aug] | **Sideways rather than up**: they change how a Perk behaves | Choosing options before the match |
| **BO3 / BO6 GobbleGum** | Meta-stock of gums from play, events and the store [Z-GG6] | Consumable power brought into a match | Consumable | Lets you save up power and spend it on a "hero" Run |

**There is a spectrum:**

1. **Unlock-only** (Gungeon, RoR2): meta-progression adds new things to the pool. Every Run starts on equal power, so records stay comparable. The risk is pool dilution, where new unlocks make the item you want rarer.
2. **Sideways choice** (BO6 Augments, Hades' red/green talent pairs): meta-progression unlocks alternatives, not strictly stronger versions.
3. **Permanent power** (Hades Mirror, VS PowerUps, Cold War tiers): veteran players are stronger. It is generous to newcomers but erodes the fresh start, and it makes Records across a save's lifetime hard to compare. Hades and VS both soften this: Hades lets you refund the Mirror and adds opt-in Heat, and VS lets you switch maxed PowerUps off.

---

## Lessons for Gauntlet

These are options and trade-offs, not decisions. Each one is a question for a later grilling ticket.

### A. Offer shape

- **Choices per offer:** 3 (genre default, quick to read) or 4 (more agency, but slower to read during an Intermission). Another option is a base of 3 with a 4th slot as something to chase, as VS and Brotato do with Luck.
- **Rarity tiers:** none (each Boon is unique, like Hades' per-god lists), or Hades-style Common to Epic with the same effect and bigger numbers. Tiers give "jackpot" moments and an obvious lever for depth-based odds, but every Boon then needs 3–4 sets of numbers.
- **Odds and depth:** fixed odds, or odds that rise with the Round number (Brotato's per-wave gain with caps [B-Shop], Gungeon's per-floor quality tables). Rising odds tie reward to how long you survive, which is the core CoD tension.
- **Rerolls, banishes and skips:** none (purest), a fixed small allowance per Run (Hades: up to 4), or paid with **Points**. Paying with Points is unique to us and creates tension with Doors, Perks and the Box, but it can starve a weak player. A skip that pays out Points is another option (VS's Skip pays XP).
- **Hidden bias:** an optional Gungeon-style Synergy Factor (nudge early picks toward Boons that combine with ones you hold) and a Magnificence-style brake (after a rare pick, rares become less likely). These are invisible and cheap, but they make balance hard to reason about.
- **Pool rules:** whether a held Boon can be offered again (as a level-up, Hades Pom-style) or leaves the pool (VS max level). Whether there are slots (VS 6 + 6) or one Boon per verb (Hades). One Boon per verb would map naturally onto Weapon fire, Reload, Knife, sprint, Lethal and Tactical.

### B. Cadence and power control

- One Boon per Round in a 30–50+ Round Run means **dozens of picks**. That is far more than Hades (about 15–25 boons per run, my estimate) and closer to VS. Options:
  - accept it and lean on structural caps (slots, Boons leaving the pool, max levels);
  - make most offers small (stat-like) and space out big ones (every 5th Round, like VS's Arcana bosses at 11:00 and 21:00 [VS-Arc] or Cold War's crystal milestones every 5 Rounds [Z-Aeth]);
  - give diminishing returns or hyperbolic stacking to anything that is a chance or a reduction [R-Stack].
- **What enemies scale off:** keep the Classic Round curve untouched, so Gauntlet is "Classic plus Boons" and easier, or add an extra difficulty factor driven by Boon count or time, like RoR2's clock [R-Diff]. Keeping Classic's curve unchanged gives the clearest comparison between modes. Adding a factor stops Gauntlet becoming a trivially easy Classic.
- **Percentage or flat:** % damage Boons keep pace with exponential Zombie Health and flat ones fade. Either is a valid deliberate choice. Fading flat Boons create natural power decay late in a Run, which is a soft cap that doesn't need its own rule.
- **Guard rails worth adopting whatever the cap model:** no linear chance stacking past a cap, no unchecked exponential stacking, and a check for loops between resource-refunding Boons [R-Stack, G-Curse].

### C. Boon categories, mapped to our systems

- **Stat:** Health, move speed, reload speed, fire rate. These overlap with Perks: Juggernog, Speed Cola and Double Tap. Should Boons stack with Perks, use their own stats, or *change* Perks (BO6-Augment-style)?
- **Weapon behaviour:** what each Weapon Class does on firing (pierce, ricochet, elemental effects), changes to Knife behaviour, Lethal and Tactical changes.
- **Economy:** Points multipliers, discounts on Buyables, Plank rewards, better Mystery Box odds. These interact strongly with Classic's economy, so they need care to avoid a snowball.
- **On-event triggers:** on Headshot, on kill, on Reload, on Down or Self-Revive, on a Power-up pickup, on a Round start. Horde play makes these fire constantly, so they need internal cooldowns.
- **System modifiers:** Power-ups (longer Insta-Kill, more Max Ammo drops), Traps (cheaper or longer), Barriers (Planks rebuild themselves), Pack-a-Punch. These make Gauntlet feel like *zombies* rather than a generic roguelite.
- **Synergies and evolutions:** optional "Boon X + Boon Y = upgrade" pairs (Hades Duos, VS evolutions). They give mid-Run goals but cost a lot of content.
- **Down interaction:** Boons surviving a Down is already decided. Still open: is there a Boon that keeps Perks through a Down (a much-wanted effect in BO4 and BO6 [Z-Elix, Z-Aug]), or is that banned because it removes Classic's Down penalty?

### D. Pausing and the choice UI

- **Hard pause** (VS) is the most readable but breaks the real-time feel of the Intermission.
- **Offer during the Intermission without pausing** (Brotato/Hades-style lull): natural, but what happens to Zombies left over and to an Intermission that runs out mid-choice? Options: hold the next Round until the player picks; auto-pick (or bank the pick) when the Intermission ends; let the player bank picks and spend them any time (BO6's 3-slot gum inventory [Z-GG6]).
- **In-world** (a Boon machine the player walks to, like the GobbleGum machine [Z-GG3]): fits the Buyable and Use key language, but Boons are "picked, never bought", so this could blur the line with Perk Machines.
- **UI:** 3–4 cards with name, effect, rarity colour and the numbers is the genre standard. Optional: show which held Boons it combines with (Gungeon's Ammonomicon highlights synergy partners [G-Syn]).

### E. Meta-progression (if any)

- **None:** every Gauntlet Run is equal and Records compare cleanly. This is the cheapest option and matches Classic's spirit.
- **Unlock-only** (Gungeon/RoR2): reaching Round milestones adds Boons to the pool. This gives long-term goals while keeping the fresh start, but it risks diluting the pool unless players can disable unlocks.
- **Sideways** (BO6 Augments, Hades red/green): unlock *variants* the player picks before the Run.
- **Permanent power** (Hades Mirror, VS PowerUps, Cold War tiers): generous, but it splits Records by save age. If chosen, the VS and Hades softeners are worth copying: refunds, toggling off, opt-in difficulty to compensate.
- **Meta-controls over the offer** (VS buys reroll and banish counts [VS-PU]; Hades buys reroll dice and rarity odds [H-Mirror]): this is meta-progression that changes the *offer*, not the Player's power, a middle ground worth considering.
- **Where Records live:** Gauntlet Records separate from Classic Records (as Hades keeps God Mode records separate [H-God]) is likely needed whatever the choice.
