# How roguelites keep a build fresh and give a Run its shape

Answers [#91](https://github.com/jchevertonwynne/zombies/issues/91), a child of the map [#90](https://github.com/jchevertonwynne/zombies/issues/90) (Gauntlet v2: a power fantasy that lasts).

**The question:** how do Hades, Vampire Survivors, Enter the Gungeon, Risk of Rain 2, Slay the Spire, Balatro, and BO6 Augments and GobbleGums handle (1) build identity, (2) synergy, (3) the shape of a run, and (4) variety between encounters? Terms such as **Boon**, **Stack**, **Pact**, **Drawback**, **Requirement**, **Rarity**, **Round**, **Intermission** and **Ammo Mod** are as defined in `GLOSSARY.md`.

This builds on [#20, Roguelite design references](https://github.com/jchevertonwynne/zombies/blob/research/roguelite-design-references/research/roguelite-design-references.md), which covered offer shape, rerolls, power brakes (slot caps, hyperbolic stacking), pause and UI, and meta-progression. None of that is repeated here except where a number is needed for comparison.

## Sources and trust

No studio here publishes a design spec, so the primary sources are the community wikis. Their numbers are mostly datamined from game files, and they quote patch history. Fandom pages were read as raw wikitext through the MediaWiki API (`/api.php?action=parse&prop=wikitext`), so table numbers are as entered, not summarised. Some claims rest on my own knowledge of the game or its decompiled code rather than on a fetched page. Each of those is marked *(my knowledge)*.

| Tag | Source |
|---|---|
| **[H-Duo]** | [Hades Wiki: Duo Boons](https://hades.fandom.com/wiki/Duo_Boons) |
| **[H-Leg]** | [Hades Wiki: Legendary Boons](https://hades.fandom.com/wiki/Legendary_Boons) |
| **[H-Boons]** | [Hades Wiki: Boons](https://hades.fandom.com/wiki/Boons) |
| **[H-Zeus]** | [Hades Wiki: Zeus/Boons (Hades)](https://hades.fandom.com/wiki/Zeus/Boons_(Hades)) |
| **[H-Pact]** | [Hades Wiki: Pact of Punishment](https://hades.fandom.com/wiki/Pact_of_Punishment) |
| **[H-Ch]** | [Hades Wiki: Chambers and Encounters](https://hades.fandom.com/wiki/Chambers_and_Encounters) |
| **[H-Tart]** | [Hades Wiki: Tartarus](https://hades.fandom.com/wiki/Tartarus), [Elysium](https://hades.fandom.com/wiki/Elysium) |
| **[H-Trial]** | [Hades Wiki: Trial of the Gods](https://hades.fandom.com/wiki/Trial_of_the_Gods) |
| **[H-Trove]** | [Hades Wiki: Infernal Trove](https://hades.fandom.com/wiki/Infernal_Trove) |
| **[H-Chaos]** | [Hades Wiki: Chaos](https://hades.fandom.com/wiki/Chaos) |
| **[VS-Evo]** | [Vampire Survivors Wiki (wiki.gg): Evolution](https://vampire.survivors.wiki/w/Evolution), [Fandom: Evolution](https://vampire-survivors.fandom.com/wiki/Evolution) |
| **[VS-Arc]** | [Vampire Survivors Wiki: Arcanas](https://vampire-survivors.fandom.com/wiki/Arcanas) |
| **[VS-LB]** | [Vampire Survivors Wiki: Limit Break](https://vampire-survivors.fandom.com/wiki/Limit_Break) |
| **[VS-Stage]** | [Vampire Survivors Wiki: Stages](https://vampire-survivors.fandom.com/wiki/Stages) (Endless, Inverse, Random Events) |
| **[G-Syn]** | [Enter the Gungeon Wiki: Synergies](https://enterthegungeon.fandom.com/wiki/Synergies) |
| **[G-Boss]** | [Enter the Gungeon Wiki: Bosses](https://enterthegungeon.fandom.com/wiki/Bosses) |
| **[G-Gun]** | [Enter the Gungeon Wiki: The Gungeon](https://enterthegungeon.fandom.com/wiki/Chambers) |
| **[G-Chal]** | [Enter the Gungeon Wiki: Challenge Mode](https://enterthegungeon.fandom.com/wiki/Challenge_Mode) |
| **[R-Proc]** | [Risk of Rain 2 Wiki: Proc Coefficient](https://riskofrain2.wiki.gg/wiki/Proc_Coefficient) |
| **[R-Items]** | [Risk of Rain 2 Wiki: Items](https://riskofrain2.wiki.gg/wiki/Items) |
| **[R-Stage]** | [Risk of Rain 2 Wiki: Stages](https://riskofrain2.wiki.gg/wiki/Stages) |
| **[R-TP]** | [Risk of Rain 2 Wiki: Teleporter](https://riskofrain2.wiki.gg/wiki/Teleporter) |
| **[R-Elite]** | [Risk of Rain 2 Wiki: Monsters § Elites](https://riskofrain2.wiki.gg/wiki/Monsters) |
| **[R-Ecl]** | [Risk of Rain 2 Wiki: Eclipse](https://riskofrain2.wiki.gg/wiki/Eclipse) |
| **[S-Asc]** | [Slay the Spire Wiki: Ascension](https://slay-the-spire.fandom.com/wiki/Ascension) |
| **[S-Elite]** | [Slay the Spire Wiki: Elites](https://slay-the-spire.fandom.com/wiki/Elites) |
| **[S-Boss]** | [Slay the Spire Wiki: Bosses](https://slay-the-spire.fandom.com/wiki/Bosses) |
| **[S-Act4]** | [Slay the Spire Wiki: Act 4](https://slay-the-spire.fandom.com/wiki/Act_4) |
| **[S-Unk]** | [Slay the Spire Wiki: Unknown Location](https://slay-the-spire.fandom.com/wiki/Unknown_Location), [Events](https://slay-the-spire.fandom.com/wiki/Events) |
| **[B-Jok]** | [Balatro Wiki: Jokers](https://balatro.fandom.com/wiki/Jokers) |
| **[B-Blind]** | [Balatro Wiki: Blinds and Antes](https://balatro.fandom.com/wiki/Blinds_and_Antes) |
| **[B-Stake]** | [Balatro Wiki: Stakes](https://balatro.fandom.com/wiki/Stakes) |
| **[B-Tag]** | [Balatro Wiki: Tags](https://balatro.fandom.com/wiki/Tags), [Vouchers](https://balatro.fandom.com/wiki/Vouchers) |
| **[Z-Aug]** | [CoD Wiki: Augments](https://callofduty.fandom.com/wiki/Augments) (BO6/BO7) |
| **[Z-GG6]** | [CoD Wiki: GobbleGum/Black Ops 6](https://callofduty.fandom.com/wiki/GobbleGum/Black_Ops_6) |
| **[Z-Exfil]** | [CoD Wiki: Exfil](https://callofduty.fandom.com/wiki/Exfil) |

---

## 1. Build identity: verbs, procs and build-defining picks

### What each game does

**Hades: every god is an element, and each boon is a verb change or a buff to that element.** Zeus's list is typical [H-Zeus]. It has six slot boons, each changing a verb ("Your Attack emits chain-lightning…", "Your Dash causes a lightning bolt…"). Two are on-event (revenge on taking damage, and on a near-miss dash). About four strengthen Zeus's own effect wherever it came from ("chain-lightning bounces more times", "lightning bolt effects have a chance to strike twice", "lightning effects also make foes Jolted"). Only about two are close to plain stats (the God Gauge charges faster, more damage after a Call). Counted by hand from that page, under about 10% of a god's list is a plain stat pick. The second-tier boons only do anything if you already hold the god's verb boons, so **picking one god twice pushes the build toward that element**. The player can name the build ("a Zeus build") because the element shows on screen as lightning. A slot holds one god's boon, and another god can offer to *exchange* it, which raises its Rarity by one [H-Boons].

**Vampire Survivors: the weapon is the verb, the passive is the stat.** There are 6 weapon slots and 6 passive slots (#20). Each weapon is a new automatic attack with its own visual. Weapon level-ups and every passive (e.g. Spinach, Might) are number picks. So the *verb* picks come early, while slots are free, and *stat* picks dominate once they're full. Evolutions (§2) turn a weapon plus its passive into a stronger, visibly different weapon. Limit Break, once unlocked, offers endless per-weapon stat bumps instead of gold or chicken when everything is maxed [VS-LB].

**Risk of Rain 2: build identity comes from procs.** Many common items are **on-hit chance** effects: Tri-Tip Dagger "10% (+10% per stack) chance to bleed… for 240% base damage", Sticky Bomb 5% (+5%), Stun Grenade 5% (+5%, hyperbolic), Lens-Maker's Glasses 10% (+10%) crit [R-Items]. Uncommons add bigger procs: AtG Missile "10% chance to fire a missile that deals 300% (+300% per stack) TOTAL damage", Ukulele "25% chance to fire chain lightning for 80% TOTAL damage on up to 3 (+2 per stack) targets within 20m" [R-Items]. There are **on-kill** procs too: Gasoline ignites everything within 12m (+4m per stack), Will-o'-the-wisp spawns a lava pillar for 350% (+280%), Ceremonial Dagger fires 3 homing daggers at 150% (+150%) [R-Items]. Some items **trigger on a big hit**: Runald's and Kjaro's Bands fire on hits over 400% damage, then recharge for 10 s [R-Items]. Wake of Vultures **steals an elite's power** for 8 s (+5 s per stack) on killing it [R-Items].

**Proc coefficients** stop fast-firing weapons from triggering everything all the time [R-Proc]. Every skill hit carries a coefficient that multiplies the chance (and the duration) of any proc it can trigger:

| Skill | Proc coefficient [R-Proc] |
|---|---|
| Commando Double Tap | 1.0 |
| MUL-T Auto-Nailgun | 0.6 |
| Huntress Flurry | 0.7 × 3 arrows |
| Huntress Arrow Rain | 0.5 × 19 ticks |
| Engineer Carbonizer Turret | 0.6 |
| Railgunner Supercharged Railgun | 3.0 |

The wiki's worked example: Tri-Tip on the Nailgun is 6% per stack (17 Daggers to guarantee a bleed), but on the Supercharged Railgun 4 Daggers are enough. Bleed lasts 3 s × coefficient, so 1.8 s or 9 s [R-Proc]. The **items' own hits carry coefficients too**: Ukulele 0.2, AtG 1.0, Will-o'-the-wisp 1.0, and Sticky Bomb and Gasoline 0.0, so they can never trigger anything else. Bleed, Burn and Collapse ticks are all 0.0 [R-Proc]. That is how RoR2 lets procs trigger other procs without an endless chain. Separately, the game marks each chain so an item can't trigger itself within it *(my knowledge)*. The coefficient is the knob that lets a slow, heavy weapon and a fast, light one use the same on-hit item at the same overall rate.

**Balatro: the joker is the build.** There are 150 Jokers (105 unlocked from the start) at 70% Common, 25% Uncommon and 5% Rare, with Legendaries only from The Soul card [B-Jok]. A run normally has **5 Joker slots** [B-Jok]. A hand scores Chips × Mult, so jokers fall into a small set of jobs: +Chips, +Mult, ×Mult, economy, retriggers, and *rule changers* that alter what counts as a hand (e.g. Four Fingers, Shortcut, Pareidolia, Splash) *(categories are my knowledge; the terms "Retrigger", "Create", "Gains" are defined in [B-Jok])*. The rule changers are Balatro's verbs: they change which hands you play. ×Mult jokers are the multiplicative layer that late Antes need. **Slots, not the pool, are the brake.** With only 5, every joker you add is also a choice about which one to sell.

**Slay the Spire: cards are verbs, relics are mostly passive.** A card reward is pick one of 3 or skip *(my knowledge)*. Relics drop from elites, chests, shops and bosses, and are mostly passive or on-event (the wiki's elite page lists Sling of Courage, "+2 Strength at the start of each elite combat", and Preserved Insect, "elites start with 25% less HP") [S-Elite]. Identity comes from what the deck is built around (Strength, Poison, Shivs, Orbs and so on), and **boss relics** at the end of acts 1 and 2 (pick one of 3) are the big build-shaping picks [S-Boss].

**BO6 Augments: a Major changes behaviour, a Minor tweaks a number.** Each Perk, Ammo Mod and Field Upgrade has 3 Major and 3 Minor Augments, and you equip one of each [Z-Aug]. Most Majors change behaviour. Jugger-Nog has "Turtle Shell: armor… completely absorbing damage to your back" and "Reactive Armor: when an armor plate breaks, nearby normal enemies are stunned". Dead Wire has "Chain Lightning: the stunned enemy can spread the stun", "Big Game: Dead Wire can stun elite enemies" and "Lightning Strike: a bolt of lightning strikes from above". Most Minors are small numbers: "High Voltage: Dead Wire deals slightly more damage", "Haste: cooldown is slightly reduced", "Durable Plates: slightly increase armor durability" [Z-Aug]. Every Ammo Mod has a **"Big Game" Augment that lets its effect work on elite enemies** [Z-Aug]. That is an upgrade defined against a threat.

**BO6 GobbleGums: temporary, in your hand, used when you choose.** You get one free every 10 s, hold up to 3, and activate them at will. They come in 3 kinds, Instant, Conditional and Timed, shown by the gum's background pattern. Many spawn a Power-up ("Kill Joy: spawns an Insta-Kill", "Cache Back: spawns a Max Ammo") [Z-GG6]. These are short-term tactics, not build identity.

### Patterns

- **The games with the strongest identity tie each upgrade to a visible family.** Hades does it by god and element, VS by weapon and its evolution, BO6 by Perk or Ammo Mod with its Augments, and RoR2 by proc type (bleed, burn, lightning, missiles). The family is the name the player gives the build.
- **Verb picks come early, stat picks fill in late.** VS (slots fill), Hades (once the 5 verb slots are full, offers turn to Poms and second-tier boons; *my knowledge*), and BO6's Major vs Minor split all have this shape. Pure-stat picks are filler in all of them, never the bulk of the offer.
- **On-hit and on-kill procs suit hordes**, but they need a throttle. RoR2 uses proc coefficients and items whose own hits can't trigger anything. BO3 Ammo Mods (our existing ones) use cooldowns: per-Player and global cooldowns that start only on a trigger, and Ammo Mod damage that never rolls again (see `research/ammo-mod-numbers.md`).
- **Upgrades defined against a threat** (BO6 "Big Game", RoR2 Wake of Vultures, StS elite relics) give the threats a reason to exist from the build's point of view.

## 2. Synergy: how rare it is and how it's surfaced

| Game | Synergy kind | Count | Requirement | How the player learns about it |
|---|---|---|---|---|
| **Hades** | Duo Boons (two gods) | **28** across 8 gods; every pair of those gods shares exactly one. Hermes and Chaos have none [H-Duo] | One qualifying boon from *each* god, e.g. Heart Rend needs an Aphrodite core boon *and* an Artemis crit boon [H-Duo]. Counted as "Tier 3", with Legendaries [H-Leg] | It shows up as a normal offer card, with a distinct border and both gods named. Odds rise with Keepsakes, Gods' Legacy, the Yarn of Ariadne and Eurydice's Nectar [H-Duo]. Codex entries show the prerequisites *(my knowledge)* |
| **Hades** | Legendary Boons (one god, deep) | **12** across 10 gods (Poseidon and Hermes have 2 each) [H-Leg] | Usually 2 specific boons from that god, e.g. Winter Harvest needs 2 of Killing Freeze, Ravenous Will and Arctic Blast [H-Leg] | As above. Mini-boss rooms raise the odds [H-Leg] |
| **Vampire Survivors** | Evolutions (weapon + passive) and Unions (2–3 weapons merge, freeing slots) | **24 evolutions** and **5 unions** in the base game's wiki tables, with dozens more from DLC [VS-Evo] | Base weapon at max level, plus its passive (most recipes), plus a Treasure Chest opened after 10:00 on most stages. An evolution is the chest's first reward. Chests evolve up to 3 weapons, or up to 5 on some stages [VS-Evo] | The Collection lists recipes once you've found them, and the level-up card shows the partner item's icon *(my knowledge; the wiki pages don't say)* |
| **Enter the Gungeon** | Synergies (named pairs or sets of guns and items) | **384** in the wiki's synergy table [G-Syn] | Holding the listed items at the same time. Effects range from stats ("Hammer and Nail": Nail Gun does triple damage, +50% max ammo) to new behaviour ("360 Yes Scope": a 360° spin gives a 3 s, ×1.5 damage buff, stacking to 6) [G-Syn] | A light-blue arrow over the player and the synergy's name in the pickup pop-up. In the Ammonomicon, selecting an item outlines its partners in blue [G-Syn]. A hidden **Synergy Factor** makes completing drops 1.8× likelier (6.0× on a fresh save), falling to 1.12 after the first synergy, 1.03 after 2 and 1.008 after 3 [G-Syn] |
| **Risk of Rain 2** | No authored synergies. Emergent ones come from procs on procs, governed by proc coefficients [R-Proc] | n/a | n/a | Not surfaced. Players discover them |
| **Balatro** | No authored pairs. Synergy comes from scoring maths (Chips × Mult × ×Mult) and joker order. Vouchers come in **16 pairs**: a $10 initial unlocks a stronger $10 dependent [B-Tag] | 32 Vouchers [B-Tag] | Voucher pairs: own the first | The shop shows a Voucher's tier. Jokers rely on reading the text |
| **BO6 Augments** | One Major plus one Minor per Perk or Ammo Mod, picked before the match | 3 + 3 per item [Z-Aug] | Researched outside the match. Unlocked by total researched count, e.g. Quick Revive at 3, Jugger-Nog at 6, Speed Cola at 12 [Z-Aug] | Menu |

**Patterns:**

- **Two rarity regimes.** *Few and named* (Hades' 28 Duos plus 12 Legendaries, VS's 24 evolutions) means each is a goal the player can learn and chase on purpose. *Many and emergent* (Gungeon's 384, RoR2's proc stacking) gives frequent surprise but little deliberate chasing, so Gungeon adds hidden bias toward completion.
- **The chase works only when the requirement can be seen.** Hades shows requirements in the Codex and offers the Duo as a card. VS shows recipes in the Collection and evolves at a known moment (a chest after 10:00). Gungeon lights up partners in the Ammonomicon. In every case, finding the second half is a deliberate act.
- **The payoff moment is gated, not instant** in VS (wait for a chest from a boss after 10:00), and immediate in Hades (it's simply in the next offer). VS's gate turns the evolution into a goal for the *middle* of the run.
- **Unions free slots** (VS): merging 2–3 weapons into one leaves room for a new verb, so late-run picks aren't all stats.
- **Our existing pieces:** Gauntlet already has **Requirements** (e.g. "Requires used the Box once", "holds an Upgraded Weapon"), which is the same mechanism as a Hades Duo prerequisite. The Boon offer just doesn't yet say *why* a Boon appeared or what it builds toward.

## 3. Run arc: acts, elites, bosses, wins and what comes after

### Structure per game

| Game | Shape of a run | Elites and mini-bosses | Bosses | Win |
|---|---|---|---|---|
| **Hades** | 4 regions: Tartarus (boss at chamber 14), Asphodel (boss at 24), Elysium (11 chambers, boss at 36, stairway at 37), then the Temple of Styx and the final boss [H-Ch, H-Tart]. Up to 73 chambers can be seen in a run [H-Ch] | One mini-boss room per region, marked by a skull on the door. Tartarus has 3 possible (Doomstone, Wretched Sneak, 2 armoured Bloodless) and you meet at most one per run [H-Tart]. Reward: a boon of raised Rarity [H-Tart] | One boss at the end of each region, chosen from a set (Tartarus picks 1 of 3 Furies) [H-Tart]. The first win per boss and per Heat gives a Titan Blood bounty, otherwise 50 Darkness [H-Tart] | Beat Hades and reach the surface. The run ends |
| **Slay the Spire** | 3 acts of 15 map floors plus a boss. Act 4 covers floors 52–55 (rest site, merchant, Spire Shield and Spear elite, Corrupt Heart) and is reached only after collecting 3 keys [S-Act4] | Elites are marked on the map. Each drops a relic, 25–35 gold and a card reward, and the same elite can't come twice in a row [S-Elite]. 3 elites per act. In a run aiming for Act 4, one elite per act is a "burning" elite with an extra buff, and it drops the Emerald Key [S-Elite] | One per act, with its portrait **shown on the map from the start** "so the player can plan their deck" [S-Boss]. The node before it is always a rest site [S-Boss]. Acts 1 and 2 bosses give a pick of 3 rare cards and a pick of 3 boss relics, and heal you to full [S-Boss] | Beat the Act 3 boss, or the Heart in Act 4 |
| **Risk of Rain 2** | Stages 1–5, then the run loops back to stage 1 environments with harder enemies; or go to Commencement (the Moon, final boss Mithrix). 16 main environments [R-Stage] | Elites are a modifier on any monster. Tier 1 (Blazing, Overloading, Glacial, Mending, Gilded): mostly ×4 health, ×2 damage. Tier 2 (Malachite, Celestine, Twisted, Collective): ×18 health, ×6 damage, from **after stage 5**, so only once you loop [R-Elite] | Each stage ends with a **Teleporter event**: you hold a ~120 m zone for at least 90 s while a boss spawns. The boss drops one Uncommon per player, with a 15% chance per drop of being the boss's own item instead [R-TP] | Kill Mithrix, or loop forever |
| **Enter the Gungeon** | 5 chambers (Keep, Gungeon Proper, Black Powder Mine, Hollow, Forge), plus secret floors and Bullet Hell [G-Gun] | n/a | One boss per floor, drawn from 2–3 options. A hitless kill drops a **Master Round** (+1 heart container) and double Hegemony Credits [G-Boss] | Beat the High Dragun, then kill your Gungeoneer's Past |
| **Vampire Survivors** | 30:00 per stage, then the Reaper comes [VS-Stage]. Boss waves drop Treasure Chests, and Arcana bosses come at **11:00 and 21:00** (you pick one of 4 Arcanas, or 6 once 23 or more are unlocked) [VS-Arc] | Chest-carrying elite enemies on a timeline [VS-Evo] | The Reaper as a hard stop | Survive to 30:00 |
| **Balatro** | 8 Antes, each 3 Blinds: Small (1× base), Big (1.5×) and Boss (usually 2×). The base requirement goes 300, 800, 2,000, 5,000, 11,000, 20,000, 35,000, **50,000** at Ante 8 on White Stake [B-Blind] | Boss Blinds are the "elites": each changes the rules (e.g. The Hook discards 2 cards after every hand, The Club debuffs every Club, The Wall needs 4× base). Each has a minimum Ante, and none repeats until the pool is used up [B-Blind] | **Finisher Blinds** at Ante 8 [B-Blind] | Beat Ante 8 |
| **CoD Zombies (Cold War, BO6)** | Open-ended Rounds | Special and elite enemies, spaced in by Round | Some maps have a quest boss | **Exfil**: from Round 10 (Cold War) or Round 11 (BO6), you call it, reach the landing zone within the time limit and clear the area. If you fail, the Run ends as though you'd died [Z-Exfil] |

### After a win

| Game | Post-win difficulty | Levels | How it works |
|---|---|---|---|
| **Hades** | **Heat** (Pact of Punishment, unlocked by the first escape) [H-Pact] | **15 Conditions** (16 in Hell Mode). Ranks sum to **63 Heat** (64 with Hell Mode's extra) [H-Pact] | A menu of conditions you compose yourself. Examples: Hard Labor +20% enemy damage per rank (5 ranks); Jury Summons +20% enemies per rank (3); **Extreme Measures** gives each region's boss new moves (4 ranks, worth 1/2/3/4 Heat); Benefits Package adds **Perks** to armoured foes, from 11 such as Shifter, Bruiser, Puller and Savior (2 ranks); Middle Management puts an extra armoured foe in every mini-boss; **Approval Process: −1 boon choice per rank** (2 ranks); Underworld Customs makes you give up a boon to leave each region; Tight Deadline gives 9 min per region, minus 2 min per rank [H-Pact]. Bounties are earned **per weapon** at a target Heat [H-Pact] |
| **Slay the Spire** | **Ascension** | **20 levels**, each unlocked by winning the one below, per character, and cumulative [S-Asc] | Fixed ladder. A1 makes elites **~60% more common** (room weight ×1.6, *my knowledge of the decompiled source*). Then deadlier normals (A2), elites (A3) and bosses (A4); 75% boss heal (A5); start at 90% health (A6); tougher normals, elites and bosses (A7–A9); a curse at the start (A10); −1 potion slot (A11); fewer upgraded cards (A12, 12.5%/25% in Acts 2/3); −25% boss gold (A13); lower max HP (A14); worse events (A15); shops +10% (A16); **new movesets for normals, elites and bosses (A17–A19)**; **two bosses in a row at the end of Act 3 (A20)**. +5% score per level [S-Asc] |
| **Risk of Rain 2** | **Eclipse** | **8 levels**, per survivor, cumulative; clearing a run raises it by one [R-Ecl] | E1 start at half health; E2 −50% teleporter radius; E3 lethal, doubled fall damage; E4 +40% enemy move speed; E5 −50% healing; E6 −20% enemy gold; E7 −50% enemy cooldowns; E8 permanent max-HP curse from damage taken [R-Ecl] |
| **Balatro** | **Stakes**, and **Endless** | **8 Stakes**, per deck, cumulative [B-Stake]. Endless starts at Ante 9 | Red: Small Blind pays no money. Green and Purple: the score needed grows faster (Ante 8 needs 100k, then 200k). Black, Orange and Gold: 30% of jokers come with **Eternal** (can't be sold), **Perishable** (debuffed after 5 Rounds) or **Rental** ($3 per Round) stickers. Blue: −1 discard [B-Stake]. **Endless** starts at Ante 9 and grows "faster than super-exponential": 110k at Ante 9, 560k at 10, 7.2M at 11, 300M at 12, 47B at 13, until it overflows to `naneinf` at **Ante 39**, the practical end [B-Blind] |
| **Vampire Survivors** | **Endless**, **Inverse**, **Limit Break**, **Random Events** | Toggles per stage | Endless: the waves restart every cycle, and each cycle gives enemies **+100% base HP, +50% spawn frequency and amount, and +25% damage** [VS-Stage]. Inverse: enemies start with +200% HP and gain +5% HP and +0.5 speed per minute, in exchange for +200% gold [VS-Stage]. Random Events: a roulette rolls a random effect every minute, which can help, hurt or do nothing much [VS-Stage] |
| **Enter the Gungeon** | **Challenge Mode** (and Double Challenge) | 1–3 modifiers per room by floor, more in Double [G-Chal] | Room modifiers such as "Explosive Pyres: enemies explode upon death", "Don't Blink: enemies you look at are stunned, the rest are jammed and faster". The Hollow and Forge bosses get their own modifiers [G-Chal] |

### Patterns: pacing difficulty against power

- **A run is made of chunks with a fixed beat**: normal → mini-boss or elite → boss, repeated 3–5 times. Hades: 3 regions of about 10–14 chambers and a boss each. StS: 3 acts of 15 floors. RoR2: 5 stages, each ending at the Teleporter. Balatro: 8 Antes, each ending in a Boss Blind. Gungeon: 5 floors. VS uses a clock instead (bosses at 10:00, 11:00, 21:00, then 30:00).
- **The boss is a check on the build, and its reward reshapes the build.** StS boss relics (pick 1 of 3) and rare cards. RoR2 Teleporter boss items. Hades raised-Rarity rewards from mini-bosses. Balatro shows the next Boss Blind's rule *in advance*, so you can prepare (or reroll it with Director's Cut or Retcon) [B-Blind]. StS shows the act's boss portrait from the start [S-Boss]. **Showing the threat ahead of time is what turns build choices into preparation.**
- **The difficulty curve runs ahead of the power curve on purpose, in steps.** Balatro's Ante requirements grow by about 2.5× per Ante early and then explode in Endless, so a build must scale multiplicatively (×Mult) or die. RoR2's tier-2 elites (×18 health) appear only after stage 5. VS Endless adds +100% HP per cycle. Every game puts the *exponential* wall *after* the win, where an endless mode measures how far a broken build can go, and keeps the main run beatable.
- **Elites are the main tool for "something pushing back".** StS elites (3 per act, marked, optional but rewarding), RoR2 elite affixes (on-hit effects that punish specific builds, e.g. Malachite stops healing, Celestine makes allies invisible), and Hades' armoured foes, which gain Perks under Benefits Package. Each is a known enemy with a known twist that a build either answers or doesn't.
- **What comes after a win is either a composable menu or a fixed ladder.** Hades Heat is a menu: the player picks which pain to take, so it stays fresh and fits each weapon. StS Ascension, RoR2 Eclipse and Balatro Stakes are fixed, cumulative ladders: easy to read and good for bragging rights, but each step is set. Hades also offers per-weapon bounties, so climbing has a reward attached. All four track progress **per character, deck or weapon**, which multiplies the content.
- **An optional exit** (CoD Exfil from Round 10 or 11) is how the zombies genre gives an open-ended Run a win condition. You choose to cash out, and the escape is itself a timed set piece that can fail [Z-Exfil].

## 4. Variety between encounters

| Game | Choosing the route | Challenge rooms | Run modifiers |
|---|---|---|---|
| **Hades** | **Every door shows its reward** before you enter: a gold laurel for in-run rewards (a specific god's Boon, Pom, Daedalus Hammer, Centaur Heart +25 HP, 100 Obols) or a blue one for meta resources. A **skull** marks a harder room with a better reward: doubled Pom strength and meta resources, higher boon Rarity [H-Ch]. Fated Authority rerolls a door's reward within its pool [H-Ch] | **Trial of the Gods**: pick one of 2 gods. The spurned one sends waves of armoured enemies with its power, then gives a boon anyway. 0–2 per run, at least 17 chambers apart [H-Trial]. **Infernal Trove**: the reward decays the longer you take to kill the guards [H-Trove]. **Survival**: survive 45 s against growing spawns [H-Ch]. **Thanatos**: beat his kill count for a Centaur Heart [H-Ch]. **Erebus gates**: clear the waves without being hit for an amplified reward [H-Ch]. **Chaos gates**: a Chaos boon is a curse for some encounters, then a blessing [H-Chaos] | Heat (above) |
| **Slay the Spire** | A visible branching map per act. Rooms: monster, elite, rest, merchant, treasure, and **? (unknown)**. ? rolls monster 10%, shop 3% or treasure 2%, each **rising by its base each time it misses** and resetting when it hits; otherwise it's an event [S-Unk]. Map rules: a 7-wide, 15-floor grid; floor 1 is all monsters, floor 9 is all treasure, floor 15 is all rest sites; no elites or rests before floor 6; room weights shop 5%, rest 12%, event 22%, elite 8% *(my knowledge of the decompiled source)* | Elites (optional, marked, rewarding) [S-Elite]. Burning elite for the Emerald Key [S-Elite]. Events: 16 can appear in more than one act, the rest are tied to an act [S-Unk] | Ascension. Daily climbs and Custom mode modifiers, e.g. Deadly Events puts elites in ? rooms [S-Unk] |
| **Risk of Rain 2** | You choose when to activate the Teleporter. Looting longer means more items but a higher, time-driven difficulty (#20). Hidden realms: the Bazaar, Void Fields, Gold Portal, and Path of the Colossus as an alternate route [R-Stage] | **Shrine of the Mountain**: each activation doubles the Teleporter boss's director credits *and* its item drops [R-TP]. Void Fields | **Artifacts**: run-wide toggles such as Command, which lets you pick items (#20), and Sacrifice *(my knowledge)* |
| **Balatro** | **Skip a Small or Big Blind for a Tag**: 24 Tags such as a free Uncommon Joker in the next shop, or a free Voucher. Some are blocked in Ante 1 [B-Tag]. The next Boss Blind is shown in advance and can be rerolled with a Voucher [B-Blind] | The Boss Blind is the challenge | Stakes. Seeded runs and Challenge decks *(my knowledge)* |
| **Enter the Gungeon** | A room-by-room layout, keys, shops and secret rooms *(my knowledge)* | Shrines that trade risk for reward, e.g. Curse (#20) | Challenge Mode: 1–3 random room modifiers per room [G-Chal]. Rainbow Mode *(my knowledge)* |
| **Vampire Survivors** | No route. Stages differ by map and wave script | Arcana bosses at 11:00 and 21:00 [VS-Arc] | Arcanas (chosen at 0:00, 11:00 and 21:00) change how the run plays, e.g. a card that makes every weapon heal. Hyper, Inverse, Endless and Random Events toggles [VS-Stage, VS-Arc] |
| **CoD Zombies** | The Doors you open choose the route, and they're permanent | Quests, boss fights on some maps, Exfil | GobbleGums used during play [Z-GG6] |

**Patterns:**

- **Show the reward and the danger, then let the player choose.** Hades doors (a reward icon plus a skull), StS map nodes and Balatro's Blind preview all give *planned* choices instead of blind ones. The skull trade (harder room, better reward) is the smallest unit of "something pushing back".
- **Bias the unknown so it evens out.** StS ? rooms raise each outcome's chance every time it misses. Gungeon's Synergy Factor does the same for drops. Both make the random feel fair.
- **Challenge rooms are short self-contained twists with their own scoring**: a time-decaying reward (Hades Trove), a hitless requirement (Erebus, Gungeon's Master Round), outscoring an NPC (Thanatos), a pick-a-side fight (Trial of the Gods), or survival on a timer (Survival, 45 s). Each one reuses existing enemies with a single new rule.
- **Skips as rewards** (Balatro Tags for skipping a Blind; #20 noted VS's Skip pays XP) turn "less fighting" into a choice with a price.
- **Run modifiers come in two kinds:** chosen before the run (Heat, Ascension, Stakes, Artifacts, Arcanas at 0:00), or rolled during it (Gungeon Challenge Mode rooms, VS Random Events, Arcanas at 11:00 and 21:00).

---

## Lessons for Gauntlet v2

These are options and trade-offs, not decisions. Each one is a question for a later grilling ticket on map #90. The current Gauntlet already offers a Boon after every Round, with **Rarity**, **Stacks**, **Requirements**, and **Pacts** every fifth Round, and the Classic Round curve does the pushing back.

### A. Build identity

- **Families, the "god" axis.** Group Boons into named families, each built on a visible effect we already have: Shock (Dead Wire, the Sapper), Fire (Blast Furnace), Knife, Explosives (Fireworks, Lethals), Thunder Wall, Turned, Economy. Each family has a few **verb Boons** (change Weapon fire, Reload, Knife, sprint, Lethal or Tactical) and a few **amplifiers** that only work if you hold the family's effect, like Hades' Zeus list.
  - *For:* the Player can name the build, and the visuals already exist.
  - *Against:* every family needs about 6–10 Boons to read as a family. A family nobody picks wastes content, so it needs bias (Section B).
- **Slot per verb (Hades) or open Stacks (current).** One Boon per verb (fire, Reload, Knife, sprint, Lethal, Tactical) forces trade-offs and makes "exchange" offers possible: replace your Reload Boon with another family's and gain a Rarity step.
  - *For:* clear identity, and late offers become decisions.
  - *Against:* it overturns the "no caps or removal" rule from #23, and it means fewer Stacks.
- **What share of picks are stats.** Stats could be kept as filler only: the 3rd card, or the reward when a family is complete, as VS turns to gold and chicken once full. Or they could disappear in favour of BO6-style Minors that hang off a verb ("Dead Wire chain +1 target").
  - *Trade-off:* stats are cheap to make, and the Player never feels them.
- **On-hit and on-kill procs.** These suit a horde. Options for the throttle:
  - (a) BO3-style cooldowns, per-Player and global, starting only on a trigger. The code already exists for Ammo Mods.
  - (b) RoR2-style **proc coefficient per Weapon Class**, so a shotgun pellet or an SMG bullet triggers less than a sniper round.
  - (c) The proc's own damage never triggers again (RoR2's 0.0 coefficient for Bleed, Burn and Sticky Bomb; BO3's `MOD_UNKNOWN` rule).
  - (a) is the simplest and matches Classic. (b) is the fairest across Weapon Classes, but needs a number per class. (c) is cheap and prevents loops whichever of the others is chosen.
- **Big Game-style Boons that target threats** ("Shock also stuns Special Zombies or Elites") tie the build to Section C. They cost little, but they only matter if threats exist.

### B. Synergy

- **How many authored synergies:** few and named (Hades: 28 Duos plus 12 Legendaries; VS: 24 evolutions), or many and emergent (Gungeon: 384; RoR2: none authored). For a small team the realistic range is a handful per family pair, or one capstone per family.
- **Kinds:**
  - **Duo** (Boon from family A plus Boon from family B, offered like a normal card). This reuses the existing **Requirement** mechanism.
  - **Evolution** (a maxed Stack plus a partner Boon turns into a new Boon at a set moment, e.g. a boss Round's reward). This overturns #23's "no evolutions", and the gate creates a mid-Run goal.
  - **Set bonus** (hold N Boons of a family to get a bonus, a Gungeon or RoR2-style count). This is the cheapest to build and the easiest to read on a HUD.
  - **Union** (merge two Boons to free a slot). This only matters if slots exist.
- **Surfacing:** show what a Boon *builds toward* on the offer card (a partner icon, as Gungeon's Ammonomicon does), list recipes in the Armory once discovered (VS Collection), announce completion (Gungeon's blue arrow).
  - *Trade-off:* more UI. The VFX, HUD and audio work is already listed as "Not yet specified" on the map.
- **Bias toward completion:** an optional Gungeon-style hidden Synergy Factor (1.8× falling to 1×), or a visible one ("a Boon that completes a Duo is guaranteed in the next boss Round's offer"). The hidden one is invisible and fair-feeling. The visible one is honest and plannable. #20 already covered the cost of hidden bias.

### C. Run arc and pushing back

- **Chunking:** split the Run into **Acts of N Rounds**, each ending in a **set-piece Round**:
  - **Act length:** Hades has about 10–14 chambers per region, StS 15 floors per act, Balatro 3 Blinds per Ante, and Pacts already sit every 5th Round. Candidates are 5-Round or 10-Round Acts.
  - **Set-piece:** a boss Zombie, an Elite wave, a Trial-style "pick a side" Round, or a Survival Round.
  - The **reward** for the set-piece could be a bigger offer (raised Rarity, a Duo or evolution chance, a pick of 3 "boss Boons" as in StS).
- **Show the threat ahead of time.** Show the next set-piece's twist during the Intermission or at the start of the Act (StS boss portrait, Balatro's Boss Blind preview), so the build can prepare. A reroll for the threat is optional (Balatro Director's Cut).
- **Elites as the pushback:** RoR2-style **affix modifiers** on ordinary Zombies (e.g. a Shock-resistant Zombie, a Zombie that stops Health regenerating on hit, an armoured Zombie that has to be knifed), introduced by Act. RoR2's tier-2 affixes come only after the first loop.
  - *For:* this reuses Zombies and Special Zombies, and lets each family meet a counter.
  - *Against:* ADR 0002 says spawning affixed Zombies needs a new seam, not a `Mode` branch in Classic.
- **A win condition:** options are:
  - (a) a fixed final Round with a boss (StS, Hades);
  - (b) a **Gauntlet Exfil**: from a set Round onward, the Player may call an escape set piece that wins the Run if they survive it, as in Cold War and BO6 from Round 10 or 11;
  - (c) no end, but Acts with a "win" banner (VS's 30:00 with Endless toggled on).
  - (b) fits the zombies genre and keeps open-ended play for those who want it. (a) gives the clearest shape. #33's "no Round cap" would be overturned by (a) and kept by (b) and (c).
- **What comes after a win:**
  - **Composable Heat** (Hades: 15 Conditions, 63 Heat; each Condition is a Pact-like Drawback taken before the Run). This fits our existing **Drawback** vocabulary well: Gauntlet Heat could reuse Drawbacks as Conditions.
  - **A fixed ladder** (StS 20 Ascensions, RoR2 8 Eclipses, Balatro 8 Stakes).
  - **Endless** after the win, with an explosive curve (Balatro, VS +100% HP per cycle).
  - Heat is richer and stays fresh. A ladder is cheaper to balance and easier to show in Records. Endless is nearly free, because Classic's Zombie Health curve is already exponential from Round 10.
  - Whatever the choice, the map's open item "Records and Lifetime Stats for wins and Heat" needs to know which.
- **Difficulty against power:** every reference puts its steepest wall after the win and keeps the main run beatable. Options:
  - keep Classic's Zombie Health curve and add pushback only through set pieces and Elites;
  - add a Gauntlet-only multiplier per Act;
  - tie the extra pushback to Boon count, like RoR2's coefficient.
  - This feeds the map's "Balance targets" item. #34's "no balance targets" would have to go if a win condition must be reachable.

### D. Variety between Rounds

- **Route choice in a zombies Map:** the Map's Doors are already a permanent route. Gauntlet options:
  - (a) **Round choice in the Intermission**: pick the next Round's twist from 2–3 cards, each showing its reward and a skull for danger, as with Hades doors;
  - (b) **Zone rewards**: opening a Door shows which reward the Zone's next clear gives;
  - (c) no route, just set pieces on a schedule.
  - (a) is the cheapest and closest to Hades. (b) uses Map geometry, but conflicts with Classic's Door economy.
- **Challenge Rounds** (each is an existing Round plus one rule):
  - Survival for N seconds (Hades: 45 s);
  - a reward that decays the longer the Round takes (Infernal Trove);
  - no Downs and no hits for a bonus (Erebus, Gungeon's Master Round);
  - Trial of the Gods: two families on offer, and the one you refuse sends an Elite wave with its effect;
  - a Chaos-style bargain, cursed for N Rounds and then blessed. This sits close to the existing **Pact**.
- **Skipping as a reward:** let the Player skip a Boon Offer, or a challenge Round, for Points, Scrip or a Tag-like token (Balatro Tags, VS Skip). This overturns #22's "no skips". It costs little and gives weak offers a use.
- **Biased randomness:** if Round twists are random, StS-style rising chances (+base on every miss, reset on a hit) keep any one twist from being absent for long.
- **Run modifiers:**
  - chosen before the Run (Heat Conditions or Ascension, VS Arcana at the start);
  - rolled during the Run (an Arcana-style pick at set Rounds, like VS at 11:00 and 21:00; Gungeon Challenge Mode per Round).
  - Pacts every 5th Round are already a rolled-during-the-Run modifier. They could become the Act boundary marker.
