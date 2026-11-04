//! Boons: the table every Boon is a row of, and the pure rules for Stacks and for drawing an
//! Offer. A Boon whose effect is a stat is one row of `BOONS`; one that reacts to Classic is a row
//! plus a small system in the Gauntlet plugin.

use bevy::prelude::*;

use super::pact::{COLD_STEEL, ENDLESS_BELT, LIGHTNING_ROD, OPEN_BAR, OVERCLOCKED, PactRef};
use crate::modifier::{Effect, Modifier, Source, Stat};
use crate::power_up::PowerUp;
use crate::tuning::*;
use crate::zombie::{Attack, Pay};

/// How often a Boon turns up on Offers. It changes nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rarity {
    Common,
    Rare,
    Legendary,
}

impl Rarity {
    pub fn label(self) -> &'static str {
        match self {
            Rarity::Common => "COMMON",
            Rarity::Rare => "RARE",
            Rarity::Legendary => "LEGENDARY",
        }
    }

    /// Common grey-white, Rare blue, Legendary gold.
    pub fn colour(self) -> Color {
        match self {
            Rarity::Common => Color::srgb(0.85, 0.85, 0.85),
            Rarity::Rare => Color::srgb(0.3, 0.6, 1.0),
            Rarity::Legendary => Color::srgb(1.0, 0.75, 0.15),
        }
    }

    /// The base Offer weight, before Stacks.
    fn weight(self) -> f64 {
        BOON_RARITY_WEIGHTS[self as usize]
    }
}

/// What must hold, when an Offer is drawn, for a Boon to be on it. A held Boon whose Requirement
/// lapses stays held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requirement {
    PowerOn,
    /// The Pact isn't held. Once it is, the Boon leaves later Offers; Stacks held stay.
    NotPact(PactRef),
    /// The Player has bought something from a Mystery Box this Run.
    UsedBox,
    /// The Player has bought Quick Revive this Run.
    BoughtQuickRevive,
    /// A Weapon the Player carries is Upgraded.
    HoldsUpgraded,
    /// A Weapon the Player carries has an Ammo Mod.
    HoldsAmmoMod,
}

impl Requirement {
    pub fn label(self) -> String {
        match self {
            Requirement::PowerOn => "Power on".into(),
            Requirement::NotPact(pact) => format!("not {}", pact.name),
            Requirement::UsedBox => "used the Box once".into(),
            Requirement::BoughtQuickRevive => "bought Quick Revive once".into(),
            Requirement::HoldsUpgraded => "holds an Upgraded Weapon".into(),
            Requirement::HoldsAmmoMod => "holds an Ammo Mod".into(),
        }
    }

    pub fn holds(self, run: &RunFacts) -> bool {
        match self {
            Requirement::PowerOn => run.power_on,
            Requirement::NotPact(pact) => !run.held_pacts.contains(&pact.id),
            Requirement::UsedBox => run.used_box,
            Requirement::BoughtQuickRevive => run.bought_quick_revive,
            Requirement::HoldsUpgraded => run.holds_upgraded,
            Requirement::HoldsAmmoMod => run.holds_ammo_mod,
        }
    }
}

/// A card's Requirement line, or `None` for a Boon or Pact without one.
pub fn requirement_line(requirements: &[Requirement]) -> Option<String> {
    (!requirements.is_empty()).then(|| {
        let labels: Vec<_> = requirements.iter().map(|r| r.label()).collect();
        format!("Requires {}", labels.join("; "))
    })
}

/// The Classic state Requirements are checked against, read when an Offer is drawn.
#[derive(Debug, Clone, Default)]
pub struct RunFacts {
    pub power_on: bool,
    /// The stable IDs of the Pacts held.
    pub held_pacts: Vec<&'static str>,
    /// Something was bought from a Mystery Box this Run.
    pub used_box: bool,
    /// Quick Revive was bought this Run.
    pub bought_quick_revive: bool,
    /// A Weapon carried is Upgraded.
    pub holds_upgraded: bool,
    /// A Weapon carried has an Ammo Mod.
    pub holds_ammo_mod: bool,
    /// The stable IDs of the Boons and Pacts bought in the Armory.
    pub unlocked: Vec<String>,
    /// Every Boon and Pact counts as Unlocked (the F9 debug key).
    pub unlock_all: bool,
}

pub struct BoonDef {
    /// The stable ID used in save files and Run Records. The display name can change.
    pub id: &'static str,
    pub name: &'static str,
    pub rarity: Rarity,
    /// Unlocked from the first launch rather than bought in the Armory.
    pub starts_unlocked: bool,
    /// All must hold when an Offer is drawn.
    pub requirements: &'static [Requirement],
    /// One Stack's stat-modifier entries. A Boon whose effect isn't a stat leaves this empty and
    /// has a system of its own.
    pub per_stack: &'static [(Stat, Effect)],
    /// The whole effect at `n` Stacks, as the Offer cards show it.
    pub effect: fn(u32) -> String,
}

pub const BLOOD_MONEY: &str = "blood_money";
pub const TEDDYS_TITHE: &str = "teddys_tithe";
pub const WAR_BONDS: &str = "war_bonds";
pub const BOUNTY: &str = "bounty";
pub const BUTCHERS_BILL: &str = "butchers_bill";
pub const LUCKY_STRIKE: &str = "lucky_strike";

/// A per-Stack fraction `x` at `n` Stacks, as a whole percentage.
fn percent(x: f32, n: u32) -> String {
    format!("{:.0}", f64::from(x) * f64::from(n) * 100.0)
}

/// A per-Stack factor `r` at `n` Stacks (rⁿ), to two places.
fn factor(r: f32, n: u32) -> String {
    format!("{:.2}", f64::from(r).powf(f64::from(n)))
}

/// A per-Stack chance `c` at `n` Stacks (hyperbolic), as a whole percentage.
fn chance_percent(c: f64, n: u32) -> String {
    format!("{:.0}", stacked_chance(c, n) * 100.0)
}

/// "1 bullet", "3 bullets".
fn plural(n: u64, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// A Common Unlocked from the first launch.
const fn common(
    id: &'static str,
    name: &'static str,
    requirements: &'static [Requirement],
    per_stack: &'static [(Stat, Effect)],
    effect: fn(u32) -> String,
) -> BoonDef {
    BoonDef {
        id,
        name,
        rarity: Rarity::Common,
        starts_unlocked: true,
        requirements,
        per_stack,
        effect,
    }
}

/// A Rare: Unlocked from the first launch, or bought in the Armory.
const fn rare(
    id: &'static str,
    name: &'static str,
    starts_unlocked: bool,
    requirements: &'static [Requirement],
    per_stack: &'static [(Stat, Effect)],
    effect: fn(u32) -> String,
) -> BoonDef {
    BoonDef {
        id,
        name,
        rarity: Rarity::Rare,
        starts_unlocked,
        requirements,
        per_stack,
        effect,
    }
}

/// A Legendary, always bought in the Armory.
const fn legendary(
    id: &'static str,
    name: &'static str,
    requirements: &'static [Requirement],
    per_stack: &'static [(Stat, Effect)],
    effect: fn(u32) -> String,
) -> BoonDef {
    BoonDef {
        id,
        name,
        rarity: Rarity::Legendary,
        starts_unlocked: false,
        requirements,
        per_stack,
        effect,
    }
}

/// Every Boon.
pub const BOONS: &[BoonDef] = &[
    common(BLOOD_MONEY, "Blood Money", &[], &[], |n| {
        let points = BOON_BLOOD_MONEY_POINTS_PER_KILL.saturating_mul(n.into());
        format!("+{points} Points per kill")
    }),
    common(
        "hollow_points",
        "Hollow Points",
        &[],
        &[(
            Stat::Damage,
            Effect::Mult(1.0 + BOON_HOLLOW_POINTS_DAMAGE_INCREASE),
        )],
        |n| {
            format!(
                "+{}% Weapon damage",
                percent(BOON_HOLLOW_POINTS_DAMAGE_INCREASE, n)
            )
        },
    ),
    common(
        "marksmans_eye",
        "Marksman's Eye",
        &[],
        &[(
            Stat::HeadshotMultiplier,
            Effect::Flat(BOON_MARKSMANS_EYE_HEADSHOT_MULTIPLIER),
        )],
        |n| {
            let added = f64::from(BOON_MARKSMANS_EYE_HEADSHOT_MULTIPLIER) * f64::from(n);
            format!("+{added:.1}x Headshot multiplier")
        },
    ),
    common(
        "full_belt",
        "Full Belt",
        &[Requirement::NotPact(ENDLESS_BELT)],
        &[(
            Stat::MagazineSize,
            Effect::Mult(1.0 + BOON_FULL_BELT_MAGAZINE_INCREASE),
        )],
        |n| {
            format!(
                "+{}% Magazine size",
                percent(BOON_FULL_BELT_MAGAZINE_INCREASE, n)
            )
        },
    ),
    common(
        "stripper_clips",
        "Stripper Clips",
        &[Requirement::NotPact(ENDLESS_BELT)],
        &[(
            Stat::MagazinePerKill,
            Effect::Flat(BOON_STRIPPER_CLIPS_BULLETS_PER_KILL as f32),
        )],
        |n| {
            let bullets = u64::from(BOON_STRIPPER_CLIPS_BULLETS_PER_KILL.saturating_mul(n));
            format!(
                "each kill puts {} back in the Magazine",
                plural(bullets, "bullet", "bullets")
            )
        },
    ),
    common(
        "light_feet",
        "Light Feet",
        &[],
        &[(
            Stat::MoveSpeed,
            Effect::Mult(1.0 + BOON_LIGHT_FEET_MOVE_SPEED_INCREASE),
        )],
        |n| {
            format!(
                "+{}% move speed",
                percent(BOON_LIGHT_FEET_MOVE_SPEED_INCREASE, n)
            )
        },
    ),
    common(
        "second_wind",
        "Second Wind",
        &[],
        &[(
            Stat::HealthRegenDelay,
            Effect::Reduction(BOON_SECOND_WIND_REGEN_DELAY_FACTOR, 1),
        )],
        |n| {
            format!(
                "Health regen delay x{}",
                factor(BOON_SECOND_WIND_REGEN_DELAY_FACTOR, n)
            )
        },
    ),
    common(TEDDYS_TITHE, "Teddy's Tithe", &[], &[], |n| {
        format!("a Teddy Bear also pays +{} Points", teddys_tithe_points(n))
    }),
    common(
        "bottomless_cup",
        "Bottomless Cup",
        &[Requirement::NotPact(OPEN_BAR)],
        &[(
            Stat::PerkPrice,
            Effect::Reduction(BOON_BOTTOMLESS_CUP_PERK_PRICE_FACTOR, 1),
        )],
        |n| {
            format!(
                "Perk prices x{}",
                factor(BOON_BOTTOMLESS_CUP_PERK_PRICE_FACTOR, n)
            )
        },
    ),
    common(
        "long_fuse",
        "Long Fuse",
        &[],
        &[(
            Stat::PowerUpDuration,
            Effect::Flat(BOON_LONG_FUSE_DURATION_SECS),
        )],
        |n| {
            let secs = f64::from(BOON_LONG_FUSE_DURATION_SECS) * f64::from(n);
            format!("timed Power-ups last +{secs:.0} s")
        },
    ),
    common(
        "scrounger",
        "Scrounger",
        &[],
        &[(
            Stat::PowerUpsPerRound,
            Effect::Flat(BOON_SCROUNGER_POWER_UPS_PER_ROUND as f32),
        )],
        |n| {
            let extra = BOON_SCROUNGER_POWER_UPS_PER_ROUND.saturating_mul(n);
            format!("+{extra} to the max Power-ups per Round")
        },
    ),
    common(
        "short_circuit",
        "Short Circuit",
        &[Requirement::PowerOn, Requirement::NotPact(LIGHTNING_ROD)],
        &[(
            Stat::TrapCooldown,
            Effect::Reduction(BOON_SHORT_CIRCUIT_TRAP_COOLDOWN_FACTOR, 1),
        )],
        |n| {
            format!(
                "Trap cooldown x{}",
                factor(BOON_SHORT_CIRCUIT_TRAP_COOLDOWN_FACTOR, n)
            )
        },
    ),
    common(
        "union_rates",
        "Union Rates",
        &[],
        &[
            (
                Stat::PlankPoints,
                Effect::Flat(BOON_UNION_RATES_POINTS_PER_PLANK as f32),
            ),
            (
                Stat::PlankPointsCap,
                Effect::Mult(1.0 + BOON_UNION_RATES_CAP_INCREASE),
            ),
        ],
        |n| {
            let points = BOON_UNION_RATES_POINTS_PER_PLANK.saturating_mul(n.into());
            let cap = 1.0 + f64::from(BOON_UNION_RATES_CAP_INCREASE) * f64::from(n);
            format!("+{points} Points per Plank; Round cap x{cap:.0}")
        },
    ),
    common(
        "trench_raider",
        "Trench Raider",
        &[Requirement::NotPact(COLD_STEEL)],
        &[(
            Stat::KnifeDamage,
            Effect::Mult(1.0 + BOON_TRENCH_RAIDER_KNIFE_DAMAGE_INCREASE),
        )],
        |n| {
            format!(
                "+{}% Knife damage",
                percent(BOON_TRENCH_RAIDER_KNIFE_DAMAGE_INCREASE, n)
            )
        },
    ),
    common(
        "bandolier",
        "Bandolier",
        &[],
        &[
            (
                Stat::LethalCapacity,
                Effect::Flat(BOON_BANDOLIER_LETHALS_CARRIED as f32),
            ),
            (
                Stat::LethalRestock,
                Effect::Flat(BOON_BANDOLIER_LETHALS_PER_ROUND as f32),
            ),
        ],
        |n| {
            let lethals = u64::from(BOON_BANDOLIER_LETHALS_CARRIED.saturating_mul(n));
            format!(
                "+{} carried and restocked per Round",
                plural(lethals, "Lethal", "Lethals")
            )
        },
    ),
    rare(WAR_BONDS, "War Bonds", true, &[], &[], |n| {
        let points = BOON_WAR_BONDS_POINTS_PER_ROUND.saturating_mul(n.into());
        format!("at Round end, +{points} x Round number Points")
    }),
    rare(
        "loaded_dice",
        "Loaded Dice",
        false,
        &[Requirement::UsedBox],
        &[(
            Stat::BoxFreeSpinChance,
            Effect::Chance(BOON_LOADED_DICE_FREE_SPIN_CHANCE, 1),
        )],
        |n| {
            format!(
                "{}% chance a spin is free",
                chance_percent(f64::from(BOON_LOADED_DICE_FREE_SPIN_CHANCE), n)
            )
        },
    ),
    rare(
        "field_engineer",
        "Field Engineer",
        false,
        &[Requirement::PowerOn],
        &[(
            Stat::PackAPunchPrice,
            Effect::Reduction(BOON_FIELD_ENGINEER_PRICE_FACTOR, 1),
        )],
        |n| {
            format!(
                "Pack-a-Punch and Repack prices x{}",
                factor(BOON_FIELD_ENGINEER_PRICE_FACTOR, n)
            )
        },
    ),
    rare(
        "live_wire",
        "Live Wire",
        false,
        &[Requirement::HoldsAmmoMod, Requirement::NotPact(OVERCLOCKED)],
        &[(
            Stat::AmmoModCooldown,
            Effect::Reduction(BOON_LIVE_WIRE_COOLDOWN_FACTOR, 1),
        )],
        |n| {
            format!(
                "Ammo Mod cooldowns x{}",
                factor(BOON_LIVE_WIRE_COOLDOWN_FACTOR, n)
            )
        },
    ),
    rare(
        "iron_stomach",
        "Iron Stomach",
        false,
        &[Requirement::NotPact(OPEN_BAR)],
        &[(
            Stat::PerksKeptOnDown,
            Effect::Flat(BOON_IRON_STOMACH_PERKS_KEPT as f32),
        )],
        |n| {
            let perks = u64::from(BOON_IRON_STOMACH_PERKS_KEPT.saturating_mul(n));
            format!(
                "a Down leaves {}, earliest bought first",
                plural(perks, "Perk", "Perks")
            )
        },
    ),
    rare(
        "last_rites",
        "Last Rites",
        false,
        &[Requirement::BoughtQuickRevive],
        &[(
            Stat::QuickReviveBuys,
            Effect::Flat(BOON_LAST_RITES_QUICK_REVIVE_BUYS as f32),
        )],
        |n| {
            let buys = u64::from(BOON_LAST_RITES_QUICK_REVIVE_BUYS.saturating_mul(n));
            format!(
                "+{} beyond the solo {QUICK_REVIVE_SOLO_BUYS}",
                plural(buys, "Quick Revive buy", "Quick Revive buys")
            )
        },
    ),
    rare(BOUNTY, "Bounty", false, &[Requirement::PowerOn], &[], |n| {
        format!("Trap kills pay {} Points", bounty_points(Attack::Trap, n))
    }),
    rare(
        "barbed_wire",
        "Barbed Wire",
        true,
        &[],
        &[(
            Stat::PlankTearDamage,
            Effect::Flat(BOON_BARBED_WIRE_HEALTH_FRACTION),
        )],
        |n| {
            format!(
                "a Zombie loses {}% of its max Health for each Plank it tears off",
                percent(BOON_BARBED_WIRE_HEALTH_FRACTION, n)
            )
        },
    ),
    rare(BUTCHERS_BILL, "Butcher's Bill", false, &[], &[], |n| {
        format!(
            "{}% chance a Knife kill drops a Power-up, outside the per-Round max",
            chance_percent(BOON_BUTCHERS_BILL_DROP_CHANCE, n)
        )
    }),
    rare(
        "shrapnel",
        "Shrapnel",
        true,
        &[],
        &[
            (
                Stat::EquipmentDamage,
                Effect::Mult(1.0 + BOON_SHRAPNEL_DAMAGE_INCREASE),
            ),
            (
                Stat::EquipmentRadius,
                Effect::Mult(1.0 + BOON_SHRAPNEL_RADIUS_INCREASE),
            ),
        ],
        |n| {
            format!(
                "+{}% explosive Equipment damage, +{}% radius",
                percent(BOON_SHRAPNEL_DAMAGE_INCREASE, n),
                percent(BOON_SHRAPNEL_RADIUS_INCREASE, n)
            )
        },
    ),
    legendary(
        "third_hand",
        "Third Hand",
        &[],
        &[(
            Stat::WeaponSlots,
            Effect::Flat(BOON_THIRD_HAND_WEAPON_SLOTS as f32),
        )],
        |n| {
            let slots = u64::from(BOON_THIRD_HAND_WEAPON_SLOTS.saturating_mul(n));
            format!(
                "+{}; slots and the Weapons in them survive a Down",
                plural(slots, "Weapon slot", "Weapon slots")
            )
        },
    ),
    legendary(
        "through_and_through",
        "Through and Through",
        &[],
        &[(
            Stat::Penetration,
            Effect::Flat(BOON_THROUGH_AND_THROUGH_ZOMBIES as f32),
        )],
        |n| {
            let zombies = BOON_THROUGH_AND_THROUGH_ZOMBIES.saturating_mul(n);
            let noun = if zombies == 1 { "Zombie" } else { "Zombies" };
            format!("bullets pass through +{zombies} more {noun}, on top of the penetration tiers")
        },
    ),
    legendary(
        "black_market",
        "Black Market",
        &[Requirement::PowerOn],
        &[(
            Stat::BoxUpgradeChance,
            Effect::Chance(BOON_BLACK_MARKET_UPGRADE_CHANCE, 1),
        )],
        |n| {
            format!(
                "{}% chance a Box Weapon comes out Upgraded",
                chance_percent(f64::from(BOON_BLACK_MARKET_UPGRADE_CHANCE), n)
            )
        },
    ),
    legendary(
        "double_punched",
        "Double-Punched",
        &[Requirement::HoldsUpgraded],
        &[(
            Stat::AmmoModSlots,
            Effect::Flat(BOON_DOUBLE_PUNCHED_AMMO_MOD_SLOTS as f32),
        )],
        |n| {
            let slots = u64::from(BOON_DOUBLE_PUNCHED_AMMO_MOD_SLOTS.saturating_mul(n));
            format!(
                "+{} on every Upgraded Weapon",
                plural(slots, "Ammo Mod slot", "Ammo Mod slots")
            )
        },
    ),
    legendary(LUCKY_STRIKE, "Lucky Strike", &[], &[], |n| {
        format!(
            "{}% chance that picking up a Power-up also gives a random different one",
            chance_percent(BOON_LUCKY_STRIKE_CHANCE, n)
        )
    }),
];

/// One Stack's effect at `n` Stacks: flat amounts `n` times over, increases added up within the
/// Boon, reductions applied `n` times (×rⁿ) and chances rolled `n` times (hyperbolic, in
/// `resolve`).
pub fn stacked(effect: Effect, n: u32) -> Effect {
    match effect {
        Effect::Flat(x) => Effect::Flat(x * n as f32),
        Effect::Mult(x) => Effect::Mult(1.0 + (x - 1.0) * n as f32),
        Effect::Reduction(r, k) => Effect::Reduction(r, k.saturating_mul(n)),
        Effect::Chance(c, k) => Effect::Chance(c, k.saturating_mul(n)),
    }
}

/// The Boons a Player holds this Run with their Stacks, in pick order. Unlimited, never removed, and kept
/// through a Down.
#[derive(Component, Default)]
pub struct HeldBoons(pub Vec<(&'static BoonDef, u32)>);

impl HeldBoons {
    pub fn stacks(&self, id: &str) -> u32 {
        self.0
            .iter()
            .find(|(boon, _)| boon.id == id)
            .map_or(0, |(_, n)| *n)
    }

    /// One more Stack of `boon`, or its first.
    pub fn add(&mut self, boon: &'static BoonDef) {
        match self.0.iter_mut().find(|(held, _)| held.id == boon.id) {
            Some((_, n)) => *n = n.saturating_add(1),
            None => self.0.push((boon, 1)),
        }
    }

    /// The stat-modifier entries every held Boon puts on the Player. Each Boon is its own source.
    pub fn modifiers(&self) -> Vec<Modifier> {
        self.0
            .iter()
            .flat_map(|&(boon, n)| {
                boon.per_stack.iter().map(move |&(stat, effect)| Modifier {
                    stat,
                    source: Source(boon.name),
                    effect: stacked(effect, n),
                })
            })
            .collect()
    }
}

/// Whether a stat-modifier entry comes from a Boon.
pub fn is_boon_source(source: Source) -> bool {
    BOONS.iter().any(|boon| Source(boon.name) == source)
}

/// A Boon's Offer weight: its Rarity's, ×0.98 for each Stack held.
pub fn offer_weight(rarity: Rarity, stacks: u32) -> f64 {
    rarity.weight() * BOON_STACK_WEIGHT_FACTOR.powf(f64::from(stacks))
}

/// Whether a Boon can be on an Offer: Unlocked, with its Requirement holding now.
pub fn eligible(boon: &BoonDef, run: &RunFacts) -> bool {
    (boon.starts_unlocked || run.unlock_all || run.unlocked.iter().any(|id| id == boon.id))
        && boon
            .requirements
            .iter()
            .all(|requirement| requirement.holds(run))
}

/// A weighted draw of up to `size` items without replacement. `roll(total)` gives a number in
/// `[0, total)`.
pub fn draw<T>(mut pool: Vec<(T, f64)>, size: usize, mut roll: impl FnMut(f64) -> f64) -> Vec<T> {
    let mut drawn = Vec::new();
    while drawn.len() < size && !pool.is_empty() {
        let total: f64 = pool.iter().map(|(_, weight)| weight).sum();
        if total <= 0.0 {
            break;
        }
        let mut left = roll(total);
        let index = pool
            .iter()
            .position(|(_, weight)| {
                left -= weight;
                left < 0.0
            })
            .unwrap_or(pool.len() - 1);
        drawn.push(pool.remove(index).0);
    }
    drawn
}

/// A Boon Offer: up to `BOON_OFFER_SIZE` distinct eligible Boons from `table`, weighted by Rarity
/// and Stacks held. Empty means no Offer.
pub fn draw_offer(
    table: &'static [BoonDef],
    held: &HeldBoons,
    run: &RunFacts,
    roll: impl FnMut(f64) -> f64,
) -> Vec<&'static BoonDef> {
    let pool = table
        .iter()
        .filter(|boon| eligible(boon, run))
        .map(|boon| (boon, offer_weight(boon.rarity, held.stacks(boon.id))))
        .collect();
    draw(pool, BOON_OFFER_SIZE, roll)
}

/// A card's Stack line: NEW, or the Stacks before and after the pick.
pub fn stack_line(stacks: u32) -> String {
    if stacks == 0 {
        "NEW".into()
    } else {
        format!("HELD x{stacks} -> x{}", stacks.saturating_add(1))
    }
}

/// A card's effect line: the effect after the pick, with the effect now before it once held.
pub fn effect_line(boon: &BoonDef, stacks: u32) -> String {
    let after = (boon.effect)(stacks.saturating_add(1));
    if stacks == 0 {
        after
    } else {
        format!("{} -> {after}", (boon.effect)(stacks))
    }
}

/// Blood Money's Points for a kill. Like kill Points, an Unpaid Kill pays none.
pub fn blood_money_points(pay: Pay, stacks: u32) -> u64 {
    match pay {
        Pay::Paid => BOON_BLOOD_MONEY_POINTS_PER_KILL.saturating_mul(stacks.into()),
        Pay::Unpaid => 0,
    }
}

/// Teddy's Tithe's Points for a Teddy Bear, on top of the refund.
pub fn teddys_tithe_points(stacks: u32) -> u64 {
    BOON_TEDDYS_TITHE_POINTS.saturating_mul(stacks.into())
}

/// A chance `c` rolled once per Stack, combined hyperbolically: `1 - (1 - c)^n`.
pub fn stacked_chance(c: f64, stacks: u32) -> f64 {
    1.0 - (1.0 - c).powf(f64::from(stacks))
}

/// War Bonds' Points at the end of Round `round`.
pub fn war_bonds_points(round: u32, stacks: u32) -> u64 {
    BOON_WAR_BONDS_POINTS_PER_ROUND
        .saturating_mul(round.into())
        .saturating_mul(stacks.into())
}

/// Bounty's Points for a kill: only a Trap's pays.
pub fn bounty_points(attack: Attack, stacks: u32) -> u64 {
    match attack {
        Attack::Trap => BOON_BOUNTY_POINTS_PER_TRAP_KILL.saturating_mul(stacks.into()),
        _ => 0,
    }
}

/// Whether Butcher's Bill drops a Power-up for a kill: only a Knife kill can, on a `roll` in
/// [0, 1) under its chance.
pub fn butchers_bill_drops(attack: Attack, stacks: u32, roll: f64) -> bool {
    attack == Attack::Knife && roll < stacked_chance(BOON_BUTCHERS_BILL_DROP_CHANCE, stacks)
}

/// Lucky Strike's gift for picking up `taken`: on a `roll` in [0, 1) under its chance, one of the
/// other five Power-ups, picked evenly by `pick` in [0, 1).
pub fn lucky_strike_gift(taken: PowerUp, stacks: u32, roll: f64, pick: f64) -> Option<PowerUp> {
    if roll >= stacked_chance(BOON_LUCKY_STRIKE_CHANCE, stacks) {
        return None;
    }
    let others: Vec<_> = PowerUp::ALL
        .into_iter()
        .filter(|&other| other != taken)
        .collect();
    let i = ((pick * others.len() as f64) as usize).min(others.len() - 1);
    Some(others[i])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modifier::{resolve, resolve_count, resolve_points};

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn named(id: &str) -> &'static BoonDef {
        BOONS.iter().find(|boon| boon.id == id).unwrap()
    }

    const fn test_boon(
        id: &'static str,
        rarity: Rarity,
        starts_unlocked: bool,
        requirements: &'static [Requirement],
    ) -> BoonDef {
        BoonDef {
            id,
            name: id,
            rarity,
            starts_unlocked,
            requirements,
            per_stack: &[],
            effect: |n| format!("x{n}"),
        }
    }

    const TABLE: &[BoonDef] = &[
        test_boon("common", Rarity::Common, true, &[]),
        test_boon("rare", Rarity::Rare, true, &[]),
        test_boon("powered", Rarity::Legendary, true, &[Requirement::PowerOn]),
        test_boon("locked", Rarity::Common, false, &[]),
    ];

    #[test]
    fn rarity_sets_the_offer_weight() {
        assert_eq!(offer_weight(Rarity::Common, 0), 60.0);
        assert_eq!(offer_weight(Rarity::Rare, 0), 30.0);
        assert_eq!(offer_weight(Rarity::Legendary, 0), 10.0);
    }

    #[test]
    fn each_stack_held_shaves_two_percent_off_the_weight() {
        assert!(close(offer_weight(Rarity::Common, 1), 58.8));
        assert!(close(offer_weight(Rarity::Rare, 2), 30.0 * 0.98 * 0.98));
        assert!(close(
            offer_weight(Rarity::Legendary, 10),
            10.0 * 0.98_f64.powi(10)
        ));
        // It shrinks towards zero but never goes below, however many are held.
        assert!(offer_weight(Rarity::Legendary, u32::MAX) >= 0.0);
    }

    #[test]
    fn the_draw_lands_by_weight() {
        let pool = vec![('a', 60.0), ('b', 30.0), ('c', 10.0)];
        // 95% of the way along the 100 total lands in c's slice; then 95% of 90 in b's.
        assert_eq!(draw(pool.clone(), 3, |total| total * 0.95), ['c', 'b', 'a']);
        assert_eq!(draw(pool.clone(), 1, |total| total * 0.5), ['a']);
        assert_eq!(draw(pool, 1, |total| total * 0.7), ['b']);
    }

    #[test]
    fn the_draw_never_repeats() {
        let pool: Vec<_> = (0..10).map(|i| (i, 1.0)).collect();
        assert_eq!(draw(pool, 3, |_| 0.0), [0, 1, 2]);
    }

    #[test]
    fn a_small_pool_draws_fewer_and_an_empty_one_none() {
        assert_eq!(draw(vec![('a', 1.0), ('b', 1.0)], 3, |_| 0.0).len(), 2);
        assert!(draw(Vec::<(char, f64)>::new(), 3, |_| 0.0).is_empty());
    }

    #[test]
    fn only_unlocked_boons_whose_requirement_holds_are_offered() {
        let held = HeldBoons::default();
        let ids = |power_on| {
            let run = RunFacts {
                power_on,
                ..default()
            };
            let mut ids: Vec<_> = draw_offer(TABLE, &held, &run, |_| 0.0)
                .into_iter()
                .map(|boon| boon.id)
                .collect();
            ids.sort();
            ids
        };
        assert_eq!(ids(false), ["common", "rare"]);
        assert_eq!(ids(true), ["common", "powered", "rare"]);
    }

    #[test]
    fn no_eligible_boons_means_no_offer() {
        const LOCKED: &[BoonDef] = &[test_boon("locked", Rarity::Common, false, &[])];
        let offer = draw_offer(LOCKED, &HeldBoons::default(), &RunFacts::default(), |_| 0.0);
        assert!(offer.is_empty());
    }

    #[test]
    fn held_stacks_lower_a_boons_weight_in_the_draw() {
        let mut held = HeldBoons::default();
        held.add(&TABLE[0]);
        held.add(&TABLE[0]);
        let mut first_total = None;
        draw_offer(TABLE, &held, &RunFacts::default(), |total| {
            first_total.get_or_insert(total);
            0.0
        });
        assert!(close(first_total.unwrap(), 60.0 * 0.98 * 0.98 + 30.0));
    }

    #[test]
    fn stacks_count_up_in_pick_order() {
        let mut held = HeldBoons::default();
        held.add(named("light_feet"));
        held.add(named(BLOOD_MONEY));
        held.add(named("light_feet"));
        let order: Vec<_> = held.0.iter().map(|(boon, n)| (boon.id, *n)).collect();
        assert_eq!(order, [("light_feet", 2), (BLOOD_MONEY, 1)]);
        assert_eq!(held.stacks("light_feet"), 2);
        assert_eq!(held.stacks("hollow_points"), 0);
    }

    #[test]
    fn stacks_follow_the_stacking_rules() {
        assert_eq!(stacked(Effect::Flat(10.0), 3), Effect::Flat(30.0));
        let Effect::Mult(x) = stacked(Effect::Mult(1.1), 3) else {
            panic!("a Mult stays a Mult");
        };
        assert!((x - 1.3).abs() < 1e-6);
        assert_eq!(
            stacked(Effect::Reduction(0.85, 1), 2),
            Effect::Reduction(0.85, 2)
        );
        assert_eq!(stacked(Effect::Chance(0.15, 1), 3), Effect::Chance(0.15, 3));
    }

    #[test]
    fn a_boons_stacks_add_and_other_sources_multiply_with_them() {
        let mut held = HeldBoons::default();
        for _ in 0..3 {
            held.add(named("hollow_points"));
        }
        let entries = held.modifiers();
        assert!((resolve(Stat::Damage, 1.0, &entries) - 1.3).abs() < 1e-6);

        // Another source (a Pact's ×2, say) multiplies: 1.3 × 2.
        let mut with_pact = entries.clone();
        with_pact.push(Modifier {
            stat: Stat::Damage,
            source: Source("Glass Jaw"),
            effect: Effect::Mult(2.0),
        });
        assert!((resolve(Stat::Damage, 1.0, &with_pact) - 2.6).abs() < 1e-6);
    }

    #[test]
    fn every_boon_is_its_own_source() {
        let mut held = HeldBoons::default();
        held.add(named("hollow_points"));
        held.add(named("light_feet"));
        let entries = held.modifiers();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|entry| is_boon_source(entry.source)));
        assert_ne!(entries[0].source, entries[1].source);
        assert!(!is_boon_source(Source("Juggernog")));
    }

    #[test]
    fn blood_money_pays_per_stack_on_paid_kills_only() {
        assert_eq!(blood_money_points(Pay::Paid, 1), 10);
        assert_eq!(blood_money_points(Pay::Paid, 3), 30);
        assert_eq!(blood_money_points(Pay::Paid, 0), 0);
        assert_eq!(blood_money_points(Pay::Unpaid, 3), 0);
    }

    #[test]
    fn cards_show_the_stacks_and_the_effect_now_and_after() {
        assert_eq!(stack_line(0), "NEW");
        assert_eq!(stack_line(2), "HELD x2 -> x3");
        let hollow = named("hollow_points");
        assert_eq!(effect_line(hollow, 0), "+10% Weapon damage");
        assert_eq!(
            effect_line(hollow, 2),
            "+20% Weapon damage -> +30% Weapon damage"
        );
        assert_eq!(
            effect_line(named(BLOOD_MONEY), 1),
            "+10 Points per kill -> +20 Points per kill"
        );
    }

    #[test]
    fn boon_ids_and_names_are_unique() {
        for (i, a) in BOONS.iter().enumerate() {
            for b in &BOONS[i + 1..] {
                assert_ne!(a.id, b.id);
                assert_ne!(a.name, b.name);
            }
        }
    }

    /// The Player's entries with `n` Stacks of one Boon.
    fn entries(id: &str, n: u32) -> Vec<Modifier> {
        let mut held = HeldBoons::default();
        for _ in 0..n {
            held.add(named(id));
        }
        held.modifiers()
    }

    /// One stat from a base with `n` Stacks of one Boon.
    fn with(id: &str, n: u32, stat: Stat, base: f32) -> f32 {
        resolve(stat, base, &entries(id, n))
    }

    fn close32(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn hollow_points_adds_ten_percent_weapon_damage_per_stack() {
        assert!(close32(
            with("hollow_points", 1, Stat::Damage, 100.0),
            110.0
        ));
        assert!(close32(
            with("hollow_points", 3, Stat::Damage, 100.0),
            130.0
        ));
    }

    #[test]
    fn marksmans_eye_adds_half_to_the_headshot_multiplier_per_stack() {
        let m1911 = 3.5;
        assert!(close32(
            with("marksmans_eye", 1, Stat::HeadshotMultiplier, m1911),
            4.0
        ));
        assert!(close32(
            with("marksmans_eye", 3, Stat::HeadshotMultiplier, m1911),
            5.0
        ));
        // Only the head: a Weapon's damage is untouched.
        assert!(close32(with("marksmans_eye", 3, Stat::Damage, 20.0), 20.0));
    }

    #[test]
    fn full_belt_adds_twenty_percent_magazine_size_per_stack() {
        let count = |n| resolve_count(Stat::MagazineSize, 8, &entries("full_belt", n));
        assert_eq!(count(1), 10);
        assert_eq!(count(3), 13);
    }

    #[test]
    fn stripper_clips_puts_a_bullet_back_per_kill_per_stack() {
        let count = |n| resolve_count(Stat::MagazinePerKill, 0, &entries("stripper_clips", n));
        assert_eq!(count(0), 0);
        assert_eq!(count(1), 1);
        assert_eq!(count(3), 3);
    }

    #[test]
    fn light_feet_adds_five_percent_move_speed_per_stack() {
        assert!(close32(
            with("light_feet", 1, Stat::MoveSpeed, 100.0),
            105.0
        ));
        assert!(close32(
            with("light_feet", 3, Stat::MoveSpeed, 100.0),
            115.0
        ));
    }

    #[test]
    fn second_wind_multiplies_the_regen_delay_per_stack() {
        let delay = |n| {
            with(
                "second_wind",
                n,
                Stat::HealthRegenDelay,
                HEALTH_REGEN_DELAY_SECS,
            )
        };
        assert!(close32(delay(1), 3.0 * 0.85));
        assert!(close32(delay(3), 3.0 * 0.85_f32.powi(3)));
    }

    #[test]
    fn teddys_tithe_pays_five_hundred_per_stack() {
        assert_eq!(teddys_tithe_points(0), 0);
        assert_eq!(teddys_tithe_points(1), 500);
        assert_eq!(teddys_tithe_points(3), 1500);
    }

    #[test]
    fn bottomless_cup_multiplies_perk_prices_per_stack() {
        let price = |n| {
            resolve_points(
                Stat::PerkPrice,
                PERK_PRICE_SPEED_COLA,
                &entries("bottomless_cup", n),
            )
        };
        assert_eq!(price(1), 2700);
        assert_eq!(price(3), 2187);
    }

    #[test]
    fn long_fuse_adds_ten_seconds_to_timed_power_ups_per_stack() {
        assert!(close32(
            with("long_fuse", 1, Stat::PowerUpDuration, 0.0),
            10.0
        ));
        assert!(close32(
            with("long_fuse", 3, Stat::PowerUpDuration, 0.0),
            30.0
        ));
    }

    #[test]
    fn scrounger_adds_one_power_up_a_round_per_stack() {
        // From BO1's 4 a Round.
        let most = |n| resolve_count(Stat::PowerUpsPerRound, 4, &entries("scrounger", n));
        assert_eq!(most(1), 5);
        assert_eq!(most(3), 7);
    }

    #[test]
    fn short_circuit_multiplies_the_trap_cooldown_per_stack() {
        let cooldown = |n| with("short_circuit", n, Stat::TrapCooldown, TRAP_COOLDOWN_SECS);
        assert!(close32(cooldown(1), 48.0));
        assert!(close32(cooldown(3), 60.0 * 0.512));
    }

    #[test]
    fn union_rates_raises_plank_points_and_the_round_cap_per_stack() {
        use crate::barrier::{PlankRates, rebuild_cap};
        let rates = |n, round| PlankRates::resolve(round, &entries("union_rates", n));
        // (1 + n) × min(50 × round, 500).
        assert_eq!(
            rates(1, 4),
            PlankRates {
                points: 20,
                cap: 2 * rebuild_cap(4)
            }
        );
        assert_eq!(
            rates(3, 4),
            PlankRates {
                points: 40,
                cap: 4 * rebuild_cap(4)
            }
        );
        assert_eq!(rates(3, 20).cap, 2000);
    }

    #[test]
    fn trench_raider_adds_fifty_percent_knife_damage_per_stack() {
        assert!(close32(
            with("trench_raider", 1, Stat::KnifeDamage, KNIFE_DAMAGE),
            225.0
        ));
        assert!(close32(
            with("trench_raider", 3, Stat::KnifeDamage, KNIFE_DAMAGE),
            375.0
        ));
        // Weapon damage is another stat.
        assert!(close32(with("trench_raider", 3, Stat::Damage, 20.0), 20.0));
    }

    #[test]
    fn bandolier_adds_a_lethal_carried_and_restocked_per_stack() {
        use crate::equipment::FragLimits;
        let limits = |n| FragLimits::resolve(&entries("bandolier", n));
        assert_eq!(
            limits(1),
            FragLimits {
                max: 5,
                per_round: 3
            }
        );
        assert_eq!(
            limits(3),
            FragLimits {
                max: 7,
                per_round: 5
            }
        );
    }

    #[test]
    fn every_common_from_appendix_b_is_in_the_table() {
        let ids = [
            BLOOD_MONEY,
            "hollow_points",
            "marksmans_eye",
            "full_belt",
            "stripper_clips",
            "light_feet",
            "second_wind",
            TEDDYS_TITHE,
            "bottomless_cup",
            "long_fuse",
            "scrounger",
            "short_circuit",
            "union_rates",
            "trench_raider",
            "bandolier",
        ];
        for id in ids {
            let boon = named(id);
            assert_eq!(boon.rarity, Rarity::Common);
            assert!(boon.starts_unlocked, "{id} is Unlocked from the start");
            assert_ne!(boon.id, boon.name, "an ID isn't a display name");
        }
        let commons = BOONS
            .iter()
            .filter(|boon| boon.rarity == Rarity::Common)
            .count();
        assert_eq!(commons, 15);
    }

    #[test]
    fn short_circuit_needs_the_power_on_and_no_lightning_rod() {
        let short_circuit = named("short_circuit");
        let run = |power_on, held_pacts: &[&'static str]| RunFacts {
            power_on,
            held_pacts: held_pacts.to_vec(),
            ..default()
        };
        assert!(!eligible(short_circuit, &run(false, &[])));
        assert!(eligible(short_circuit, &run(true, &[])));
        assert!(!eligible(short_circuit, &run(true, &[LIGHTNING_ROD.id])));
    }

    #[test]
    fn a_held_pact_takes_the_boons_it_overrides_off_later_offers() {
        let offered = |held_pacts: Vec<&'static str>| {
            let run = RunFacts {
                power_on: true,
                held_pacts,
                ..default()
            };
            BOONS
                .iter()
                .filter(|boon| eligible(boon, &run))
                .map(|boon| boon.id)
                .collect::<Vec<_>>()
        };
        // The 15 Commons, plus the three Rares Unlocked from the start.
        assert_eq!(offered(vec![]).len(), 18);
        let without = offered(vec![ENDLESS_BELT.id]);
        assert!(!without.contains(&"full_belt"));
        assert!(!without.contains(&"stripper_clips"));
        assert_eq!(without.len(), 16);
        assert!(!offered(vec![OPEN_BAR.id]).contains(&"bottomless_cup"));
        assert!(!offered(vec![COLD_STEEL.id]).contains(&"trench_raider"));
    }

    #[test]
    fn stacks_already_held_stay_when_a_requirement_lapses() {
        let mut held = HeldBoons::default();
        held.add(named("full_belt"));
        let run = RunFacts {
            power_on: true,
            held_pacts: vec![ENDLESS_BELT.id],
            ..default()
        };
        let offer = draw_offer(BOONS, &held, &run, |_| 0.0);
        assert!(offer.iter().all(|boon| boon.id != "full_belt"));
        assert_eq!(held.stacks("full_belt"), 1);
        assert_eq!(held.modifiers().len(), 1);
    }

    #[test]
    fn cards_show_every_requirement() {
        assert_eq!(requirement_line(named("hollow_points").requirements), None);
        assert_eq!(
            requirement_line(named("short_circuit").requirements).as_deref(),
            Some("Requires Power on; not Lightning Rod")
        );
        assert_eq!(
            requirement_line(named("full_belt").requirements).as_deref(),
            Some("Requires not Endless Belt")
        );
    }

    #[test]
    fn effect_lines_read_per_stack() {
        let line = |id, n| (named(id).effect)(n);
        assert_eq!(line("marksmans_eye", 3), "+1.5x Headshot multiplier");
        assert_eq!(
            line("stripper_clips", 1),
            "each kill puts 1 bullet back in the Magazine"
        );
        assert_eq!(
            line("stripper_clips", 3),
            "each kill puts 3 bullets back in the Magazine"
        );
        assert_eq!(line("second_wind", 1), "Health regen delay x0.85");
        assert_eq!(line("bottomless_cup", 3), "Perk prices x0.73");
        assert_eq!(line(TEDDYS_TITHE, 3), "a Teddy Bear also pays +1500 Points");
        assert_eq!(line("long_fuse", 3), "timed Power-ups last +30 s");
        assert_eq!(line("scrounger", 3), "+3 to the max Power-ups per Round");
        assert_eq!(line("short_circuit", 3), "Trap cooldown x0.51");
        assert_eq!(line("union_rates", 3), "+30 Points per Plank; Round cap x4");
        assert_eq!(line("trench_raider", 3), "+150% Knife damage");
        assert_eq!(
            line("bandolier", 1),
            "+1 Lethal carried and restocked per Round"
        );
        assert_eq!(line("full_belt", 3), "+60% Magazine size");
    }
    /// Run facts with every Requirement but the Pacts holding, and every Boon Unlocked.
    fn everything() -> RunFacts {
        RunFacts {
            power_on: true,
            held_pacts: Vec::new(),
            used_box: true,
            bought_quick_revive: true,
            holds_upgraded: true,
            holds_ammo_mod: true,
            unlocked: Vec::new(),
            unlock_all: true,
        }
    }

    #[test]
    fn every_rare_and_legendary_from_appendix_b_is_in_the_table() {
        let rares = [
            (WAR_BONDS, true),
            ("loaded_dice", false),
            ("field_engineer", false),
            ("live_wire", false),
            ("iron_stomach", false),
            ("last_rites", false),
            (BOUNTY, false),
            ("barbed_wire", true),
            (BUTCHERS_BILL, false),
            ("shrapnel", true),
        ];
        for (id, starts_unlocked) in rares {
            let boon = named(id);
            assert_eq!(boon.rarity, Rarity::Rare, "{id}");
            assert_eq!(boon.starts_unlocked, starts_unlocked, "{id}");
            assert_ne!(boon.id, boon.name);
        }
        let legendaries = [
            "third_hand",
            "through_and_through",
            "black_market",
            "double_punched",
            LUCKY_STRIKE,
        ];
        for id in legendaries {
            let boon = named(id);
            assert_eq!(boon.rarity, Rarity::Legendary, "{id}");
            assert!(!boon.starts_unlocked, "{id} is bought in the Armory");
        }
        let count = |rarity| BOONS.iter().filter(|boon| boon.rarity == rarity).count();
        assert_eq!((count(Rarity::Rare), count(Rarity::Legendary)), (10, 5));
    }

    #[test]
    fn locked_boons_are_offered_only_once_unlocked() {
        let locked = RunFacts {
            unlock_all: false,
            ..everything()
        };
        assert!(!eligible(named("third_hand"), &locked));
        assert!(eligible(named("third_hand"), &everything()));
        let bought = RunFacts {
            unlocked: vec!["third_hand".into()],
            ..locked.clone()
        };
        assert!(eligible(named("third_hand"), &bought));
        assert!(!eligible(named("lucky_strike"), &bought));
        assert!(eligible(named("barbed_wire"), &locked));
        assert_eq!(
            BOONS
                .iter()
                .filter(|boon| eligible(boon, &everything()))
                .count(),
            30
        );
    }

    #[test]
    fn requirements_read_the_run() {
        let without = |boon: &str, fact: fn(&mut RunFacts)| {
            let mut run = everything();
            fact(&mut run);
            eligible(named(boon), &run)
        };
        assert!(!without("loaded_dice", |run| run.used_box = false));
        assert!(!without("field_engineer", |run| run.power_on = false));
        assert!(!without("live_wire", |run| run.holds_ammo_mod = false));
        assert!(!without("live_wire", |run| run.held_pacts = vec![OVERCLOCKED.id]));
        assert!(!without("iron_stomach", |run| run.held_pacts = vec![OPEN_BAR.id]));
        assert!(!without("last_rites", |run| run.bought_quick_revive = false));
        assert!(!without(BOUNTY, |run| run.power_on = false));
        assert!(!without("black_market", |run| run.power_on = false));
        assert!(!without("double_punched", |run| run.holds_upgraded = false));
        // Boons without a Requirement don't care.
        for id in [WAR_BONDS, "third_hand", LUCKY_STRIKE, "barbed_wire"] {
            assert!(without(id, |run| *run = RunFacts {
                unlock_all: true,
                ..default()
            }));
        }
    }

    #[test]
    fn rare_cards_show_their_requirements() {
        let line = |id| requirement_line(named(id).requirements).unwrap();
        assert_eq!(line("loaded_dice"), "Requires used the Box once");
        assert_eq!(line("last_rites"), "Requires bought Quick Revive once");
        assert_eq!(line("double_punched"), "Requires holds an Upgraded Weapon");
        assert_eq!(
            line("live_wire"),
            "Requires holds an Ammo Mod; not Overclocked"
        );
    }

    #[test]
    fn chances_stack_hyperbolically() {
        assert!(close(stacked_chance(0.15, 0), 0.0));
        assert!(close(stacked_chance(0.15, 1), 0.15));
        assert!(close(stacked_chance(0.15, 3), 1.0 - 0.85_f64.powi(3)));
        assert!(stacked_chance(0.5, 100) < 1.0 + 1e-12);
    }

    #[test]
    fn war_bonds_pays_fifty_times_the_round_per_stack() {
        assert_eq!(war_bonds_points(7, 0), 0);
        assert_eq!(war_bonds_points(7, 1), 350);
        assert_eq!(war_bonds_points(7, 3), 1050);
    }

    #[test]
    fn loaded_dice_frees_spins_hyperbolically() {
        use crate::mystery_box::free_spin;
        let chance = |n| with("loaded_dice", n, Stat::BoxFreeSpinChance, 0.0);
        assert!(close32(chance(1), 0.15));
        assert!(close32(chance(3), 1.0 - 0.85_f32.powi(3)));
        assert!(free_spin(&entries("loaded_dice", 1), 0.149));
        assert!(!free_spin(&entries("loaded_dice", 1), 0.151));
    }

    #[test]
    fn field_engineer_multiplies_pack_a_punch_and_repack_prices() {
        use crate::ammo_mod::repack_price;
        use crate::pack_a_punch::machine_price;
        let price = |base, n| machine_price(base, &entries("field_engineer", n));
        assert_eq!(price(PAP_PRICE, 1), 4500);
        assert_eq!(price(PAP_PRICE, 3), 3645);
        assert_eq!(price(repack_price(0), 1), 2250);
    }

    #[test]
    fn live_wire_multiplies_ammo_mod_cooldowns() {
        let cooldown = |n| with("live_wire", n, Stat::AmmoModCooldown, 10.0);
        assert!(close32(cooldown(1), 8.5));
        assert!(close32(cooldown(3), 10.0 * 0.85_f32.powi(3)));
    }

    #[test]
    fn iron_stomach_keeps_the_earliest_perks_through_a_down() {
        use crate::modifier::Modifiers;
        use crate::perk::{Perk, Perks, kept_on_down};
        let down = |n| {
            let mut perks = Perks(vec![
                Perk::Juggernog,
                Perk::QuickRevive,
                Perk::DoubleTap,
                Perk::SpeedCola,
            ]);
            let mut player = entries("iron_stomach", n);
            for perk in &perks.0 {
                player.extend_from_slice(perk.modifiers());
            }
            let mut modifiers = Modifiers(player);
            perks.lose_on_down(kept_on_down(&modifiers.0), &mut modifiers);
            (perks.0, modifiers.0)
        };
        let (perks, modifiers) = down(1);
        assert_eq!(perks, [Perk::Juggernog]);
        // The Boon's own entry survives too, for the next Down.
        assert_eq!(modifiers.len(), 2);
        let (perks, _) = down(3);
        assert_eq!(perks, [Perk::Juggernog, Perk::DoubleTap, Perk::SpeedCola]);
        assert!(down(0).0.is_empty());
    }

    #[test]
    fn last_rites_adds_a_quick_revive_buy_per_stack() {
        use crate::perk::quick_revive_buys;
        assert_eq!(quick_revive_buys(&entries("last_rites", 1)), 4);
        assert_eq!(quick_revive_buys(&entries("last_rites", 3)), 6);
    }

    #[test]
    fn bounty_pays_fifty_per_trap_kill_per_stack() {
        assert_eq!(bounty_points(Attack::Trap, 1), 50);
        assert_eq!(bounty_points(Attack::Trap, 3), 150);
        assert_eq!(bounty_points(Attack::Bullet, 3), 0);
        assert_eq!(bounty_points(Attack::Trap, 0), 0);
    }

    #[test]
    fn barbed_wire_costs_a_tenth_of_max_health_per_plank_per_stack() {
        use crate::barrier::plank_tear_damage;
        // Round 1 Zombies have 150 Health.
        assert!(close32(
            plank_tear_damage(1, &entries("barbed_wire", 1)),
            15.0
        ));
        assert!(close32(
            plank_tear_damage(1, &entries("barbed_wire", 3)),
            45.0
        ));
    }

    #[test]
    fn butchers_bill_drops_from_knife_kills_by_chance() {
        assert!(butchers_bill_drops(Attack::Knife, 1, 0.099));
        assert!(!butchers_bill_drops(Attack::Knife, 1, 0.101));
        assert!(butchers_bill_drops(Attack::Knife, 3, 0.27));
        assert!(!butchers_bill_drops(Attack::Knife, 3, 0.272));
        assert!(!butchers_bill_drops(Attack::Bullet, 3, 0.0));
        assert!(!butchers_bill_drops(Attack::Knife, 0, 0.0));
    }

    #[test]
    fn shrapnel_raises_equipment_damage_and_radius_per_stack() {
        use crate::equipment::equipment_splash;
        let one = equipment_splash(FRAG_SPLASH, &entries("shrapnel", 1));
        assert!(close32(one.max, FRAG_SPLASH.max * 1.25));
        assert!(close32(one.radius, FRAG_SPLASH.radius * 1.1));
        let three = equipment_splash(FRAG_SPLASH, &entries("shrapnel", 3));
        assert!(close32(three.min, FRAG_SPLASH.min * 1.75));
        assert!(close32(three.radius, FRAG_SPLASH.radius * 1.3));
    }

    #[test]
    fn third_hand_adds_a_slot_and_its_weapons_survive_a_down() {
        use crate::arsenal::WeaponId;
        use crate::last_stand::LastStand;
        use crate::modifier::resolve_count;
        use crate::weapon::Loadout;
        let slots = |n| resolve_count(Stat::WeaponSlots, WEAPON_SLOTS, &entries("third_hand", n));
        assert_eq!(slots(1), 3);
        assert_eq!(slots(3), 5);

        let mut loadout = Loadout::new(WeaponId::M1911);
        loadout.set_slot_count(slots(1) as usize);
        loadout.give(WeaponId::Mp40);
        loadout.give(WeaponId::M14);
        let mut stand = LastStand::begin(&mut loadout);
        stand.self_revive(&mut loadout);
        assert_eq!(loadout.slot_count(), 3);
        assert_eq!(loadout.weapons().count(), 3);
    }

    #[test]
    fn through_and_through_adds_a_zombie_on_top_of_the_tier_per_stack() {
        use crate::arsenal::WeaponId;
        use crate::weapon::Loadout;
        let m14 = Loadout::new(WeaponId::M14);
        let tier = WeaponId::M14.stats().penetration.extra_zombies();
        assert_eq!(
            m14.penetration(&entries("through_and_through", 1)),
            tier + 1
        );
        assert_eq!(
            m14.penetration(&entries("through_and_through", 3)),
            tier + 3
        );
    }

    #[test]
    fn black_market_upgrades_box_weapons_by_chance() {
        use crate::arsenal::WeaponId;
        use crate::mystery_box::{Prize, comes_out_upgraded};
        let chance = |n| with("black_market", n, Stat::BoxUpgradeChance, 0.0);
        assert!(close32(chance(1), 0.2));
        assert!(close32(chance(3), 1.0 - 0.8_f32.powi(3)));
        let prize = Prize::Weapon(WeaponId::Galil);
        assert!(comes_out_upgraded(prize, &entries("black_market", 1), 0.19));
        assert!(!comes_out_upgraded(
            prize,
            &entries("black_market", 1),
            0.21
        ));
    }

    #[test]
    fn double_punched_adds_an_ammo_mod_slot_per_stack() {
        use crate::ammo_mod::ammo_mod_slots;
        assert_eq!(ammo_mod_slots(&entries("double_punched", 1)), 3);
        assert_eq!(ammo_mod_slots(&entries("double_punched", 3)), 5);
    }

    #[test]
    fn lucky_strike_gives_a_different_power_up_by_chance() {
        assert_eq!(lucky_strike_gift(PowerUp::Nuke, 1, 0.21, 0.0), None);
        assert_eq!(lucky_strike_gift(PowerUp::Nuke, 0, 0.0, 0.0), None);
        // 1 − 0.8³ ≈ 0.488 at 3 Stacks.
        assert!(lucky_strike_gift(PowerUp::Nuke, 3, 0.48, 0.0).is_some());
        for taken in PowerUp::ALL {
            for i in 0..10 {
                let gift = lucky_strike_gift(taken, 1, 0.0, i as f64 / 10.0).unwrap();
                assert_ne!(gift, taken);
            }
        }
        assert_eq!(
            lucky_strike_gift(PowerUp::MaxAmmo, 1, 0.0, 0.0),
            Some(PowerUp::InstaKill)
        );
        assert_eq!(
            lucky_strike_gift(PowerUp::MaxAmmo, 1, 0.0, 0.99),
            Some(PowerUp::FireSale)
        );
    }

    #[test]
    fn rare_and_legendary_effect_lines_read_per_stack() {
        let line = |id, n| (named(id).effect)(n);
        assert_eq!(
            line(WAR_BONDS, 2),
            "at Round end, +100 x Round number Points"
        );
        assert_eq!(line("loaded_dice", 1), "15% chance a spin is free");
        assert_eq!(line("loaded_dice", 3), "39% chance a spin is free");
        assert_eq!(
            line("field_engineer", 1),
            "Pack-a-Punch and Repack prices x0.90"
        );
        assert_eq!(
            line("iron_stomach", 1),
            "a Down leaves 1 Perk, earliest bought first"
        );
        assert_eq!(
            line("iron_stomach", 2),
            "a Down leaves 2 Perks, earliest bought first"
        );
        assert_eq!(
            line("last_rites", 1),
            "+1 Quick Revive buy beyond the solo 3"
        );
        assert_eq!(line(BOUNTY, 3), "Trap kills pay 150 Points");
        assert_eq!(
            line("barbed_wire", 1),
            "a Zombie loses 10% of its max Health for each Plank it tears off"
        );
        assert_eq!(
            line("shrapnel", 2),
            "+50% explosive Equipment damage, +20% radius"
        );
        assert_eq!(
            line("third_hand", 1),
            "+1 Weapon slot; slots and the Weapons in them survive a Down"
        );
        assert_eq!(
            line("through_and_through", 2),
            "bullets pass through +2 more Zombies, on top of the penetration tiers"
        );
        assert_eq!(
            line("black_market", 1),
            "20% chance a Box Weapon comes out Upgraded"
        );
        assert_eq!(
            line("double_punched", 1),
            "+1 Ammo Mod slot on every Upgraded Weapon"
        );
        assert_eq!(
            line(LUCKY_STRIKE, 1),
            "20% chance that picking up a Power-up also gives a random different one"
        );
    }
}
