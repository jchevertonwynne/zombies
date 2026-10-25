//! Boons: the table every Boon is a row of, and the pure rules for Stacks and for drawing an
//! Offer. A Boon whose effect is a stat is one row of `BOONS`; one that reacts to Classic is a row
//! plus a small system in the Gauntlet plugin.

use bevy::prelude::*;

use crate::modifier::{Effect, Modifier, Source, Stat};
use crate::tuning::*;
use crate::zombie::Pay;

/// How often a Boon turns up on Offers. It changes nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rarity {
    Common,
    #[allow(dead_code, reason = "the Rares arrive with the roster")]
    Rare,
    #[allow(dead_code, reason = "the Legendaries arrive with the roster")]
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

/// A Pact a Requirement names, by stable ID and display name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PactRef {
    pub id: &'static str,
    pub name: &'static str,
}

// The Pacts the Commons' Requirements name. The Pacts themselves arrive in their own slice.
pub const ENDLESS_BELT: PactRef = PactRef {
    id: "endless_belt",
    name: "Endless Belt",
};
pub const OPEN_BAR: PactRef = PactRef {
    id: "open_bar",
    name: "Open Bar",
};
pub const LIGHTNING_ROD: PactRef = PactRef {
    id: "lightning_rod",
    name: "Lightning Rod",
};
pub const COLD_STEEL: PactRef = PactRef {
    id: "cold_steel",
    name: "Cold Steel",
};

/// What must hold, when an Offer is drawn, for a Boon to be on it. A held Boon whose Requirement
/// lapses stays held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requirement {
    PowerOn,
    /// The Pact isn't held. Once it is, the Boon leaves later Offers; Stacks held stay.
    NotPact(PactRef),
}

impl Requirement {
    pub fn label(self) -> String {
        match self {
            Requirement::PowerOn => "Power on".into(),
            Requirement::NotPact(pact) => format!("not {}", pact.name),
        }
    }

    fn holds(self, run: &RunFacts) -> bool {
        match self {
            Requirement::PowerOn => run.power_on,
            Requirement::NotPact(pact) => !run.held_pacts.contains(&pact.id),
        }
    }
}

/// A card's Requirement line, or `None` for a Boon without one.
pub fn requirement_line(boon: &BoonDef) -> Option<String> {
    (!boon.requirements.is_empty()).then(|| {
        let labels: Vec<_> = boon.requirements.iter().map(|r| r.label()).collect();
        format!("Requires {}", labels.join("; "))
    })
}

/// The Classic state Requirements are checked against, read when an Offer is drawn.
#[derive(Debug, Clone, Default)]
pub struct RunFacts {
    pub power_on: bool,
    /// The stable IDs of the Pacts held.
    pub held_pacts: Vec<&'static str>,
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

/// A per-Stack fraction `x` at `n` Stacks, as a whole percentage.
fn percent(x: f32, n: u32) -> String {
    format!("{:.0}", f64::from(x) * f64::from(n) * 100.0)
}

/// A per-Stack factor `r` at `n` Stacks (rⁿ), to two places.
fn factor(r: f32, n: u32) -> String {
    format!("{:.2}", f64::from(r).powf(f64::from(n)))
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

/// The Boons held this Run with their Stacks, in pick order. Unlimited, never removed, and kept
/// through a Down.
#[derive(Resource, Default)]
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
    // Until the Armory saves Unlocks, the Boons Unlocked from the first launch are the only ones.
    boon.starts_unlocked
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
            };
            BOONS
                .iter()
                .filter(|boon| eligible(boon, &run))
                .map(|boon| boon.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(offered(vec![]).len(), 15);
        let without = offered(vec![ENDLESS_BELT.id]);
        assert!(!without.contains(&"full_belt"));
        assert!(!without.contains(&"stripper_clips"));
        assert_eq!(without.len(), 13);
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
        };
        let offer = draw_offer(BOONS, &held, &run, |_| 0.0);
        assert!(offer.iter().all(|boon| boon.id != "full_belt"));
        assert_eq!(held.stacks("full_belt"), 1);
        assert_eq!(held.modifiers().len(), 1);
    }

    #[test]
    fn cards_show_every_requirement() {
        assert_eq!(requirement_line(named("hollow_points")), None);
        assert_eq!(
            requirement_line(named("short_circuit")).as_deref(),
            Some("Requires Power on; not Lightning Rod")
        );
        assert_eq!(
            requirement_line(named("full_belt")).as_deref(),
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
}
