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

/// What must hold, when an Offer is drawn, for a Boon to be on it. A held Boon whose Requirement
/// lapses stays held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requirement {
    #[allow(
        dead_code,
        reason = "the Boons that need the Power arrive with the roster"
    )]
    PowerOn,
}

impl Requirement {
    pub fn label(self) -> &'static str {
        match self {
            Requirement::PowerOn => "Power on",
        }
    }

    fn holds(self, run: &RunFacts) -> bool {
        match self {
            Requirement::PowerOn => run.power_on,
        }
    }
}

/// The Classic state Requirements are checked against, read when an Offer is drawn.
#[derive(Debug, Clone, Copy, Default)]
pub struct RunFacts {
    pub power_on: bool,
}

pub struct BoonDef {
    /// The stable ID used in save files and Run Records. The display name can change.
    pub id: &'static str,
    pub name: &'static str,
    pub rarity: Rarity,
    /// Unlocked from the first launch rather than bought in the Armory.
    pub starts_unlocked: bool,
    pub requirement: Option<Requirement>,
    /// One Stack's stat-modifier entries. A Boon whose effect isn't a stat leaves this empty and
    /// has a system of its own.
    pub per_stack: &'static [(Stat, Effect)],
    /// The whole effect at `n` Stacks, as the Offer cards show it.
    pub effect: fn(u32) -> String,
}

pub const BLOOD_MONEY: &str = "blood_money";

/// A per-Stack fraction `x` at `n` Stacks, as a whole percentage.
fn percent(x: f32, n: u32) -> String {
    format!("{:.0}", f64::from(x) * f64::from(n) * 100.0)
}

/// Every Boon.
pub const BOONS: &[BoonDef] = &[
    BoonDef {
        id: BLOOD_MONEY,
        name: "Blood Money",
        rarity: Rarity::Common,
        starts_unlocked: true,
        requirement: None,
        per_stack: &[],
        effect: |n| {
            let points = BOON_BLOOD_MONEY_POINTS_PER_KILL.saturating_mul(n.into());
            format!("+{points} Points per kill")
        },
    },
    BoonDef {
        id: "hollow_points",
        name: "Hollow Points",
        rarity: Rarity::Common,
        starts_unlocked: true,
        requirement: None,
        per_stack: &[(
            Stat::Damage,
            Effect::Mult(1.0 + BOON_HOLLOW_POINTS_DAMAGE_INCREASE),
        )],
        effect: |n| {
            format!(
                "+{}% Weapon damage",
                percent(BOON_HOLLOW_POINTS_DAMAGE_INCREASE, n)
            )
        },
    },
    BoonDef {
        id: "light_feet",
        name: "Light Feet",
        rarity: Rarity::Common,
        starts_unlocked: true,
        requirement: None,
        per_stack: &[(
            Stat::MoveSpeed,
            Effect::Mult(1.0 + BOON_LIGHT_FEET_MOVE_SPEED_INCREASE),
        )],
        effect: |n| {
            format!(
                "+{}% move speed",
                percent(BOON_LIGHT_FEET_MOVE_SPEED_INCREASE, n)
            )
        },
    },
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
            .requirement
            .is_none_or(|requirement| requirement.holds(run))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modifier::resolve;

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
        requirement: Option<Requirement>,
    ) -> BoonDef {
        BoonDef {
            id,
            name: id,
            rarity,
            starts_unlocked,
            requirement,
            per_stack: &[],
            effect: |n| format!("x{n}"),
        }
    }

    const TABLE: &[BoonDef] = &[
        test_boon("common", Rarity::Common, true, None),
        test_boon("rare", Rarity::Rare, true, None),
        test_boon(
            "powered",
            Rarity::Legendary,
            true,
            Some(Requirement::PowerOn),
        ),
        test_boon("locked", Rarity::Common, false, None),
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
            let run = RunFacts { power_on };
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
        const LOCKED: &[BoonDef] = &[test_boon("locked", Rarity::Common, false, None)];
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
}
