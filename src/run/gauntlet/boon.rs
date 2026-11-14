//! Boons: the table every Boon is a row of, and the pure rules for Stacks and for drawing an
//! Offer. A Boon whose effect is a stat is one row of `BOONS`; one that reacts to Classic is a row
//! plus a small system in the Gauntlet plugin.

use bevy::prelude::*;

use super::pact::{COLD_STEEL, ENDLESS_BELT, LIGHTNING_ROD, OPEN_BAR, OVERCLOCKED, PactRef};
use super::procs::{self, ProcDef, proc_size};
use crate::modifier::{Effect, Modifier, Source, Stat};
use crate::economy::power_up::PowerUp;
use crate::tuning::*;
use crate::enemies::zombie::{Attack, Pay};

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

/// The six Families: every Boon belongs to exactly one. Pacts are in none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Lead,
    Blade,
    Spark,
    Ember,
    Gold,
    Grit,
}

impl Family {
    pub const ALL: [Family; 6] = [Family::Lead, Family::Blade, Family::Spark, Family::Ember, Family::Gold, Family::Grit];

    pub fn name(self) -> &'static str {
        match self {
            Family::Lead => "Lead",
            Family::Blade => "Blade",
            Family::Spark => "Spark",
            Family::Ember => "Ember",
            Family::Gold => "Gold",
            Family::Grit => "Grit",
        }
    }

    pub fn colour(self) -> Color {
        let [r, g, b] = GAUNTLET_FAMILY_COLOURS[self as usize];
        Color::srgb_u8(r, g, b)
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
    /// A Weapon the Player carries has an Ammo Mod, or the Player holds a Proc Boon.
    HoldsAmmoModOrProc,
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
            Requirement::HoldsAmmoModOrProc => "holds an Ammo Mod or a Proc Boon".into(),
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
            Requirement::HoldsAmmoModOrProc => run.holds_ammo_mod || run.holds_proc_boon,
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
    /// The Power has been on this Run: the Power Switch thrown, or any Generator ever Held.
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
    /// A Proc Boon is held.
    pub holds_proc_boon: bool,
    /// The stable IDs of the Boons and Pacts bought in the Armory.
    pub unlocked: Vec<String>,
    /// Every Boon and Pact counts as Unlocked (the F9 debug key).
    pub unlock_all: bool,
}

pub struct BoonDef {
    /// The stable ID used in save files and Run Records. The display name can change.
    pub id: &'static str,
    pub name: &'static str,
    /// The one Family it belongs to.
    pub family: Family,
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
    /// A Proc Boon's Trigger and Effect.
    pub proc: Option<&'static ProcDef>,
}

pub const BLOOD_MONEY: &str = "blood_money";
pub const TEDDYS_TITHE: &str = "teddys_tithe";
pub const WAR_BONDS: &str = "war_bonds";
pub const BOUNTY: &str = "bounty";
pub const BUTCHERS_BILL: &str = "butchers_bill";
pub const LUCKY_STRIKE: &str = "lucky_strike";
pub const LIVE_WIRE: &str = "live_wire";

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
    family: Family,
    requirements: &'static [Requirement],
    per_stack: &'static [(Stat, Effect)],
    effect: fn(u32) -> String,
) -> BoonDef {
    BoonDef {
        id,
        name,
        family,
        rarity: Rarity::Common,
        starts_unlocked: true,
        requirements,
        per_stack,
        effect,
        proc: None,
    }
}

/// A Rare: Unlocked from the first launch, or bought in the Armory.
const fn rare(
    id: &'static str,
    name: &'static str,
    family: Family,
    starts_unlocked: bool,
    requirements: &'static [Requirement],
    per_stack: &'static [(Stat, Effect)],
    effect: fn(u32) -> String,
) -> BoonDef {
    BoonDef {
        id,
        name,
        family,
        rarity: Rarity::Rare,
        starts_unlocked,
        requirements,
        per_stack,
        effect,
        proc: None,
    }
}

/// A Legendary, always bought in the Armory.
const fn legendary(
    id: &'static str,
    name: &'static str,
    family: Family,
    requirements: &'static [Requirement],
    per_stack: &'static [(Stat, Effect)],
    effect: fn(u32) -> String,
) -> BoonDef {
    BoonDef {
        id,
        name,
        family,
        rarity: Rarity::Legendary,
        starts_unlocked: false,
        requirements,
        per_stack,
        effect,
        proc: None,
    }
}

/// A Proc Boon, bought in the Armory, with no Requirement.
const fn proc_boon(
    id: &'static str,
    name: &'static str,
    rarity: Rarity,
    proc: &'static ProcDef,
    effect: fn(u32) -> String,
) -> BoonDef {
    BoonDef {
        id,
        name,
        family: proc.family,
        rarity,
        starts_unlocked: false,
        requirements: &[],
        per_stack: &[],
        effect,
        proc: Some(proc),
    }
}

/// A Proc Boon's card line: what it does, then its cooldown if it has one.
fn proc_line(what: String, def: &ProcDef) -> String {
    if def.cooldown_secs > 0.0 {
        format!("{what} ({:.0} s cooldown)", def.cooldown_secs)
    } else {
        what
    }
}

/// A Proc's chance as a whole percentage.
fn proc_chance(def: &ProcDef) -> String {
    format!("{:.0}%", def.chance * 100.0)
}

/// Every Boon.
pub const BOONS: &[BoonDef] = &[
    common(BLOOD_MONEY, "Blood Money", Family::Gold, &[], &[], |n| {
        let points = BOON_BLOOD_MONEY_POINTS_PER_KILL.saturating_mul(n.into());
        format!("+{points} Points per kill")
    }),
    common(
        "hollow_points",
        "Hollow Points", Family::Lead,
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
        "Marksman's Eye", Family::Lead,
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
        "Full Belt", Family::Lead,
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
        "Stripper Clips", Family::Lead,
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
        "Light Feet", Family::Blade,
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
        "Second Wind", Family::Grit,
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
    common(TEDDYS_TITHE, "Teddy's Tithe", Family::Gold, &[], &[], |n| {
        format!("a Teddy Bear also pays +{} Points", teddys_tithe_points(n))
    }),
    common(
        "bottomless_cup",
        "Bottomless Cup", Family::Gold,
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
        "Long Fuse", Family::Gold,
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
        "Scrounger", Family::Gold,
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
        "Short Circuit", Family::Spark,
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
        "Union Rates", Family::Grit,
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
        "Trench Raider", Family::Blade,
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
        "Bandolier", Family::Ember,
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
    common(
        "long_arm",
        "Long Arm",
        Family::Blade,
        &[],
        &[(Stat::KnifeRange, Effect::Flat(BOON_LONG_ARM_KNIFE_RANGE_M))],
        |n| {
            let metres = f64::from(BOON_LONG_ARM_KNIFE_RANGE_M) * f64::from(n);
            format!("+{metres:.2} m Knife range")
        },
    ),
    common(
        "kindling",
        "Kindling",
        Family::Ember,
        &[],
        &[(
            Stat::FireRadius,
            Effect::Mult(1.0 + BOON_KINDLING_FIRE_RADIUS_INCREASE),
        )],
        |n| {
            format!(
                "+{}% radius of every fire Effect",
                percent(BOON_KINDLING_FIRE_RADIUS_INCREASE, n)
            )
        },
    ),
    common(
        "black_powder",
        "Black Powder",
        Family::Ember,
        &[],
        &[(
            Stat::EquipmentRadius,
            Effect::Mult(1.0 + BOON_BLACK_POWDER_RADIUS_INCREASE),
        )],
        |n| {
            format!(
                "+{}% explosive Equipment blast radius",
                percent(BOON_BLACK_POWDER_RADIUS_INCREASE, n)
            )
        },
    ),
    rare(WAR_BONDS, "War Bonds", Family::Gold, true, &[], &[], |n| {
        let points = BOON_WAR_BONDS_POINTS_PER_ROUND.saturating_mul(n.into());
        format!("at Round end, +{points} x Round number Points")
    }),
    common(
        "loaded_dice",
        "Loaded Dice", Family::Gold,
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
    common(
        "field_engineer",
        "Field Engineer", Family::Spark,
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
        LIVE_WIRE,
        "Live Wire", Family::Spark,
        false,
        &[Requirement::HoldsAmmoModOrProc, Requirement::NotPact(OVERCLOCKED)],
        // Proc cooldowns read its Stacks themselves (`procs::proc_cooldown`).
        &[(
            Stat::AmmoModCooldown,
            Effect::Reduction(BOON_LIVE_WIRE_COOLDOWN_FACTOR, 1),
        )],
        |n| {
            format!(
                "Ammo Mod and Proc cooldowns x{}",
                factor(BOON_LIVE_WIRE_COOLDOWN_FACTOR, n)
            )
        },
    ),
    common(
        "iron_stomach",
        "Iron Stomach", Family::Grit,
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
    common(
        "last_rites",
        "Last Rites", Family::Grit,
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
    common(BOUNTY, "Bounty", Family::Spark, &[Requirement::PowerOn], &[], |n| {
        format!("Trap kills pay {} Points", bounty_points(Attack::Trap, n))
    }),
    rare(
        "barbed_wire",
        "Barbed Wire", Family::Grit,
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
    common(BUTCHERS_BILL, "Butcher's Bill", Family::Blade, &[], &[], |n| {
        format!(
            "{}% chance a Knife kill drops a Power-up, outside the per-Round max",
            chance_percent(BOON_BUTCHERS_BILL_DROP_CHANCE, n)
        )
    }),
    rare(
        "shrapnel",
        "Shrapnel", Family::Ember,
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
        "Third Hand", Family::Lead,
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
        "Through and Through", Family::Lead,
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
        "Black Market", Family::Spark,
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
        "Double-Punched", Family::Spark,
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
    legendary(LUCKY_STRIKE, "Lucky Strike", Family::Gold, &[], &[], |n| {
        format!(
            "{}% chance that picking up a Power-up also gives a random different one",
            chance_percent(BOON_LUCKY_STRIKE_CHANCE, n)
        )
    }),
    // The Proc Boons (spec #182, Appendix A3).
    proc_boon("arc_light", "Arc Light", Rarity::Rare, &procs::ARC_LIGHT, |n| {
        let def = &procs::ARC_LIGHT;
        proc_line(format!("Headshot kill: Shock chains to {:.0} Zombies", proc_size(def, n)), def)
    }),
    proc_boon("hot_brass", "Hot Brass", Rarity::Rare, &procs::HOT_BRASS, |n| {
        let def = &procs::HOT_BRASS;
        proc_line(format!("Reload finished: a fire ring sets alight Zombies within {:.1} m", proc_size(def, n)), def)
    }),
    proc_boon("elbow_room", "Elbow Room", Rarity::Rare, &procs::ELBOW_ROOM, |n| {
        let def = &procs::ELBOW_ROOM;
        proc_line(
            format!(
                "Taking a hit: knockback kills and flings the {:.0} closest Zombies within {BOON_ELBOW_ROOM_RADIUS_M:.0} m",
                proc_size(def, n)
            ),
            def,
        )
    }),
    proc_boon("parting_shot", "Parting Shot", Rarity::Rare, &procs::PARTING_SHOT, |n| {
        let def = &procs::PARTING_SHOT;
        proc_line(format!("Magazine emptied: that shot's hit Blasts, killing within {:.1} m", proc_size(def, n)), def)
    }),
    proc_boon("frostbite", "Frostbite", Rarity::Rare, &procs::FROSTBITE, |n| {
        let def = &procs::FROSTBITE;
        proc_line(
            format!(
                "Knife kill: Frost slows every Zombie within {BOON_FROSTBITE_RADIUS_M:.0} m for {:.0} s",
                proc_size(def, n)
            ),
            def,
        )
    }),
    proc_boon("spoils_of_war", "Spoils of War", Rarity::Rare, &procs::SPOILS_OF_WAR, |n| {
        let def = &procs::SPOILS_OF_WAR;
        proc_line(format!("Power-up picked up: a phantom gun fires {:.0} shots", proc_size(def, n)), def)
    }),
    proc_boon("turncoat", "Turncoat", Rarity::Rare, &procs::TURNCOAT, |n| {
        let def = &procs::TURNCOAT;
        proc_line(
            format!("{} chance a kill Turns the nearest Zombie for {:.0} s", proc_chance(def), proc_size(def, n)),
            def,
        )
    }),
    proc_boon("firebrand", "Firebrand", Rarity::Rare, &procs::FIREBRAND, |n| {
        let def = &procs::FIREBRAND;
        proc_line(
            format!(
                "Lethal thrown: a {:.1} m fire pool where it lands, for {BOON_FIREBRAND_POOL_SECS:.0} s",
                proc_size(def, n)
            ),
            def,
        )
    }),
    proc_boon("static_discharge", "Static Discharge", Rarity::Legendary, &procs::STATIC_DISCHARGE, |n| {
        let def = &procs::STATIC_DISCHARGE;
        proc_line(format!("Taking a hit: Shock kills every Zombie within {:.0} m", proc_size(def, n)), def)
    }),
    proc_boon("wildfire", "Wildfire", Rarity::Legendary, &procs::WILDFIRE, |n| {
        let def = &procs::WILDFIRE;
        proc_line(
            format!(
                "{} chance a kill's corpse bursts, setting alight Zombies within {:.0} m",
                proc_chance(def),
                proc_size(def, n)
            ),
            def,
        )
    }),
    proc_boon("firing_squad", "Firing Squad", Rarity::Legendary, &procs::FIRING_SQUAD, |n| {
        let def = &procs::FIRING_SQUAD;
        proc_line(format!("Reload finished: a phantom gun fires {:.0} shots", proc_size(def, n)), def)
    }),
    proc_boon("raise_the_dead", "Raise the Dead", Rarity::Legendary, &procs::RAISE_THE_DEAD, |n| {
        let def = &procs::RAISE_THE_DEAD;
        let zombies = proc_size(def, n);
        proc_line(
            format!(
                "{} chance a Headshot kill raises {zombies:.0} Turned {} (at most {GAUNTLET_TURNED_ALLY_CAP} allies)",
                proc_chance(def),
                if zombies == 1.0 { "Zombie" } else { "Zombies" },
            ),
            def,
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
            family: Family::Lead,
            rarity,
            starts_unlocked,
            requirements,
            per_stack: &[],
            effect: |n| format!("x{n}"),
            proc: None,
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
        use crate::maps::barrier::{PlankRates, rebuild_cap};
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
        use crate::weapons::equipment::FragLimits;
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
            // v1 Rares moved to Common by spec #182, effects unchanged.
            "loaded_dice",
            "field_engineer",
            "iron_stomach",
            "last_rites",
            BOUNTY,
            BUTCHERS_BILL,
            // New in spec #182.
            "long_arm",
            "kindling",
            "black_powder",
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
        assert_eq!(commons, 24);
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
        // The 24 Commons but Loaded Dice and Last Rites (their Requirements don't hold), plus the
        // three Rares Unlocked from the start.
        assert_eq!(offered(vec![]).len(), 25);
        let without = offered(vec![ENDLESS_BELT.id]);
        assert!(!without.contains(&"full_belt"));
        assert!(!without.contains(&"stripper_clips"));
        assert_eq!(without.len(), 23);
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
            holds_proc_boon: true,
            unlocked: Vec::new(),
            unlock_all: true,
        }
    }

    #[test]
    fn every_rare_and_legendary_from_appendix_b_is_in_the_table() {
        let rares = [
            (WAR_BONDS, true),
            (LIVE_WIRE, false),
            ("barbed_wire", true),
            ("shrapnel", true),
            ("arc_light", false),
            ("hot_brass", false),
            ("elbow_room", false),
            ("parting_shot", false),
            ("frostbite", false),
            ("spoils_of_war", false),
            ("turncoat", false),
            ("firebrand", false),
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
            "static_discharge",
            "wildfire",
            "firing_squad",
            "raise_the_dead",
        ];
        for id in legendaries {
            let boon = named(id);
            assert_eq!(boon.rarity, Rarity::Legendary, "{id}");
            assert!(!boon.starts_unlocked, "{id} is bought in the Armory");
        }
        let count = |rarity| BOONS.iter().filter(|boon| boon.rarity == rarity).count();
        assert_eq!((count(Rarity::Rare), count(Rarity::Legendary)), (12, 9));
    }

    #[test]
    fn the_twelve_proc_boons_are_eight_rares_and_four_legendaries_with_no_requirement() {
        let procs: Vec<_> = BOONS.iter().filter(|boon| boon.proc.is_some()).collect();
        let count = |rarity| procs.iter().filter(|boon| boon.rarity == rarity).count();
        assert_eq!((count(Rarity::Rare), count(Rarity::Legendary)), (8, 4));
        for boon in procs {
            assert!(boon.requirements.is_empty(), "{}", boon.id);
            assert!(boon.per_stack.is_empty(), "{} is a Trigger and an Effect, not a stat", boon.id);
            assert!(!boon.starts_unlocked, "{} is bought in the Armory", boon.id);
        }
    }

    #[test]
    fn live_wire_needs_an_ammo_mod_or_a_proc_boon_and_no_overclocked() {
        let run = |holds_ammo_mod, holds_proc_boon, held_pacts: Vec<&'static str>| RunFacts {
            holds_ammo_mod,
            holds_proc_boon,
            held_pacts,
            unlock_all: true,
            ..default()
        };
        let live_wire = named(LIVE_WIRE);
        assert!(!eligible(live_wire, &run(false, false, vec![])));
        assert!(eligible(live_wire, &run(true, false, vec![])));
        assert!(eligible(live_wire, &run(false, true, vec![])));
        assert!(!eligible(live_wire, &run(false, true, vec![OVERCLOCKED.id])));
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
            45
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
        assert!(!without("live_wire", |run| {
            run.holds_ammo_mod = false;
            run.holds_proc_boon = false;
        }));
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
            "Requires holds an Ammo Mod or a Proc Boon; not Overclocked"
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
        use crate::weapons::mystery_box::free_spin;
        let chance = |n| with("loaded_dice", n, Stat::BoxFreeSpinChance, 0.0);
        assert!(close32(chance(1), 0.15));
        assert!(close32(chance(3), 1.0 - 0.85_f32.powi(3)));
        assert!(free_spin(&entries("loaded_dice", 1), 0.149));
        assert!(!free_spin(&entries("loaded_dice", 1), 0.151));
    }

    #[test]
    fn field_engineer_multiplies_pack_a_punch_and_repack_prices() {
        use crate::weapons::ammo_mod::repack_price;
        use crate::weapons::pack_a_punch::machine_price;
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
        use crate::economy::perk::{Perk, Perks, kept_on_down};
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
        use crate::economy::perk::quick_revive_buys;
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
        use crate::maps::barrier::plank_tear_damage;
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
        use crate::weapons::equipment::equipment_splash;
        let one = equipment_splash(FRAG_SPLASH, &entries("shrapnel", 1));
        assert!(close32(one.max, FRAG_SPLASH.max * 1.25));
        assert!(close32(one.radius, FRAG_SPLASH.radius * 1.1));
        let three = equipment_splash(FRAG_SPLASH, &entries("shrapnel", 3));
        assert!(close32(three.min, FRAG_SPLASH.min * 1.75));
        assert!(close32(three.radius, FRAG_SPLASH.radius * 1.3));
    }

    #[test]
    fn third_hand_adds_a_slot_and_its_weapons_survive_a_down() {
        use crate::weapons::arsenal::WeaponId;
        use crate::players::last_stand::LastStand;
        use crate::modifier::resolve_count;
        use crate::weapons::weapon::Loadout;
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
        use crate::weapons::arsenal::WeaponId;
        use crate::weapons::weapon::Loadout;
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
        use crate::weapons::arsenal::WeaponId;
        use crate::weapons::mystery_box::{Prize, comes_out_upgraded};
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
        use crate::weapons::ammo_mod::ammo_mod_slots;
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
        assert_eq!(line(LIVE_WIRE, 2), "Ammo Mod and Proc cooldowns x0.72");
    }

    #[test]
    fn proc_boon_cards_read_their_effect_at_each_stack() {
        let line = |id, n| (named(id).effect)(n);
        assert_eq!(line("arc_light", 1), "Headshot kill: Shock chains to 3 Zombies (2 s cooldown)");
        assert_eq!(line("arc_light", 2), "Headshot kill: Shock chains to 5 Zombies (2 s cooldown)");
        assert_eq!(
            line("hot_brass", 2),
            "Reload finished: a fire ring sets alight Zombies within 3.0 m (8 s cooldown)"
        );
        assert_eq!(line("parting_shot", 1), "Magazine emptied: that shot's hit Blasts, killing within 2.0 m");
        assert_eq!(line("turncoat", 2), "5% chance a kill Turns the nearest Zombie for 25 s (20 s cooldown)");
        assert_eq!(
            line("raise_the_dead", 1),
            "10% chance a Headshot kill raises 1 Turned Zombie (at most 3 allies) (5 s cooldown)"
        );
    }

    #[test]
    fn every_boon_is_in_its_family_from_appendix_a() {
        use Family::*;
        let families = [
            (Lead, &["hollow_points", "marksmans_eye", "full_belt", "stripper_clips", "parting_shot", "through_and_through", "third_hand", "firing_squad"][..]),
            (Blade, &["trench_raider", BUTCHERS_BILL, "light_feet", "long_arm", "frostbite", "elbow_room", "static_discharge"]),
            (Spark, &["short_circuit", "field_engineer", BOUNTY, LIVE_WIRE, "arc_light", "double_punched", "black_market"]),
            (Ember, &["bandolier", "kindling", "black_powder", "shrapnel", "hot_brass", "firebrand", "wildfire"]),
            (Gold, &[BLOOD_MONEY, TEDDYS_TITHE, "bottomless_cup", "loaded_dice", "long_fuse", "scrounger", WAR_BONDS, "spoils_of_war", LUCKY_STRIKE]),
            (Grit, &["second_wind", "iron_stomach", "last_rites", "union_rates", "barbed_wire", "turncoat", "raise_the_dead"]),
        ];
        let mut listed = 0;
        for (family, ids) in families {
            for id in ids {
                assert_eq!(named(id).family, family, "{id}");
            }
            assert!(ids.len() >= 7, "{family:?} has at least 7");
            listed += ids.len();
        }
        assert_eq!(listed, 45);
        assert_eq!(BOONS.len(), 45, "every Boon has a Family");
    }

    #[test]
    fn a_proc_boons_accent_is_its_own_family() {
        for boon in BOONS {
            if let Some(proc) = boon.proc {
                assert_eq!(proc.family, boon.family, "{}", boon.id);
            }
        }
    }

    #[test]
    fn long_arm_adds_a_quarter_metre_of_knife_range_per_stack() {
        use crate::weapons::weapon::knife_range;
        assert!(close32(knife_range(&entries("long_arm", 1)), 1.85));
        assert!(close32(knife_range(&entries("long_arm", 3)), 2.35));
    }

    #[test]
    fn kindling_adds_a_tenth_to_every_fire_radius_per_stack() {
        use crate::weapons::ammo_mod::fire_radius;
        assert!(close32(fire_radius(2.5, &entries("kindling", 1)), 2.75));
        assert!(close32(fire_radius(2.5, &entries("kindling", 3)), 3.25));
    }

    #[test]
    fn black_powder_adds_fifteen_percent_blast_radius_per_stack() {
        use crate::weapons::equipment::equipment_splash;
        let two = equipment_splash(FRAG_SPLASH, &entries("black_powder", 2));
        assert!(close32(two.radius, FRAG_SPLASH.radius * 1.3));
        assert!(close32(two.max, FRAG_SPLASH.max), "radius only");
    }
}
