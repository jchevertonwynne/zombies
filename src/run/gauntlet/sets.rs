//! Set Bonuses: a Family's tiers, reached at 2, 4 and 6 distinct held Boons of it. Stacks of one
//! Boon count once, and the tiers add up and are never lost. Tier 2 is a stat, tier 4 changes a
//! rule and tier 6 is a capstone you can see (spec #182, Appendix A4).

use bevy::prelude::*;

use super::boon::{Family, HeldBoons};
use crate::modifier::{Effect, Modifier, Source, Stat};
use crate::tuning::*;

/// The tiers a Player's Families have reached this Run: for each Family, how many of
/// `SET_BONUS_TIERS` (0 to 3). Never lowered.
#[derive(Component, Debug, Default, Clone, PartialEq, Eq)]
pub struct SetBonuses([u8; 6]);

impl SetBonuses {
    /// Whether `family` has reached tier `tier` (2, 4 or 6).
    pub fn has(&self, family: Family, tier: u32) -> bool {
        let reached = self.0[family as usize] as usize;
        SET_BONUS_TIERS[..reached].contains(&tier)
    }

    /// Raises each Family to the tiers its distinct held Boons reach, returning every tier newly
    /// reached as (Family, tier), lowest first.
    pub fn reach(&mut self, held: &HeldBoons) -> Vec<(Family, u32)> {
        let mut reached = Vec::new();
        for family in Family::ALL {
            let was = self.0[family as usize];
            let now = tiers_for(held.in_family(family));
            if now > was {
                reached.extend(SET_BONUS_TIERS[was as usize..now as usize].iter().map(|&tier| (family, tier)));
                self.0[family as usize] = now;
            }
        }
        reached
    }
}

/// Each tier's own source, so it never adds into a Boon's.
const SOURCES: [[Source; 3]; 6] = [
    [Source("Lead 2"), Source("Lead 4"), Source("Lead 6")],
    [Source("Blade 2"), Source("Blade 4"), Source("Blade 6")],
    [Source("Spark 2"), Source("Spark 4"), Source("Spark 6")],
    [Source("Ember 2"), Source("Ember 4"), Source("Ember 6")],
    [Source("Gold 2"), Source("Gold 4"), Source("Gold 6")],
    [Source("Grit 2"), Source("Grit 4"), Source("Grit 6")],
];

/// The source a Family's tier (2, 4 or 6) puts its entries under.
fn source(family: Family, tier: u32) -> Source {
    let index = SET_BONUS_TIERS.iter().position(|&held| held == tier).unwrap_or(0);
    SOURCES[family as usize][index]
}

/// Whether a stat-modifier entry comes from a Set Bonus.
pub fn is_set_source(source: Source) -> bool {
    SOURCES.iter().flatten().any(|&held| held == source)
}

/// Whether a Magazine holding `magazine` of `size` is in its last third, for Lead 4.
pub fn magazine_low(magazine: u32, size: u32) -> bool {
    f64::from(magazine) <= f64::from(size) * SET_LEAD_4_MAGAZINE_FRACTION
}

/// The stat-modifier entries the tiers reached put on the Player: every tier 2's stat, and Lead 4's
/// damage while `magazine_low`.
pub fn set_entries(sets: &SetBonuses, magazine_low: bool) -> Vec<Modifier> {
    let mut entries = Vec::new();
    let mut add = |family, tier, stat, effect| {
        if sets.has(family, tier) {
            entries.push(Modifier { stat, source: source(family, tier), effect });
        }
    };
    add(Family::Lead, 2, Stat::HeadshotMultiplier, Effect::Flat(SET_LEAD_2_HEADSHOT_MULTIPLIER));
    add(Family::Blade, 2, Stat::KnifeDamage, Effect::Mult(1.0 + SET_BLADE_2_KNIFE_DAMAGE_INCREASE));
    add(Family::Spark, 2, Stat::AmmoModCooldown, Effect::Mult(SET_SPARK_2_COOLDOWN_FACTOR));
    add(Family::Spark, 2, Stat::TrapCooldown, Effect::Mult(SET_SPARK_2_COOLDOWN_FACTOR));
    add(Family::Ember, 2, Stat::EquipmentDamage, Effect::Mult(1.0 + SET_EMBER_2_DAMAGE_INCREASE));
    add(Family::Ember, 2, Stat::BurnSpeed, Effect::Mult(SET_EMBER_2_BURN_SPEED_FACTOR));
    add(Family::Gold, 2, Stat::PointsRate, Effect::Flat(SET_GOLD_2_POINTS_RATE));
    add(Family::Grit, 2, Stat::RoundRegen, Effect::Mult(SET_GRIT_2_REGEN_FACTOR));
    if magazine_low {
        add(Family::Lead, 4, Stat::Damage, Effect::Mult(1.0 + SET_LEAD_4_DAMAGE_INCREASE));
    }
    entries
}

/// How many of `SET_BONUS_TIERS` `distinct` held Boons of one Family reach.
pub fn tiers_for(distinct: u32) -> u8 {
    SET_BONUS_TIERS.iter().filter(|&&tier| distinct >= tier).count() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::gauntlet::boon::BOONS;
    use crate::modifier::resolve;

    fn named(id: &str) -> &'static super::super::boon::BoonDef {
        BOONS.iter().find(|boon| boon.id == id).unwrap()
    }

    fn holding(ids: &[&str]) -> HeldBoons {
        let mut held = HeldBoons::default();
        for id in ids {
            held.add(named(id));
        }
        held
    }

    const LEAD: [&str; 6] = ["hollow_points", "marksmans_eye", "full_belt", "stripper_clips", "parting_shot", "third_hand"];

    #[test]
    fn a_familys_tiers_come_at_two_four_and_six_distinct_boons() {
        assert_eq!(
            (0..=7).map(tiers_for).collect::<Vec<_>>(),
            [0, 0, 1, 1, 2, 2, 3, 3]
        );
    }

    #[test]
    fn stacks_of_one_boon_count_once_toward_a_tier() {
        let mut sets = SetBonuses::default();
        let held = holding(&["hollow_points", "hollow_points", "hollow_points"]);
        assert!(sets.reach(&held).is_empty());
        assert!(!sets.has(Family::Lead, 2));
    }

    #[test]
    fn each_tier_is_reported_once_as_it_is_reached() {
        let mut sets = SetBonuses::default();
        assert_eq!(sets.reach(&holding(&LEAD[..2])), [(Family::Lead, 2)]);
        assert!(sets.reach(&holding(&LEAD[..3])).is_empty());
        assert_eq!(sets.reach(&holding(&LEAD[..4])), [(Family::Lead, 4)]);
        assert!(sets.has(Family::Lead, 2) && sets.has(Family::Lead, 4));
        assert!(!sets.has(Family::Lead, 6));
        assert!(!sets.has(Family::Blade, 2), "other Families are their own");
    }

    #[test]
    fn several_tiers_reached_at_once_are_each_reported() {
        let mut sets = SetBonuses::default();
        assert_eq!(sets.reach(&holding(&LEAD)), [(Family::Lead, 2), (Family::Lead, 4), (Family::Lead, 6)]);
    }

    #[test]
    fn a_tier_reached_is_never_lost() {
        let mut sets = SetBonuses::default();
        sets.reach(&holding(&LEAD));
        assert!(sets.reach(&HeldBoons::default()).is_empty());
        assert!(sets.has(Family::Lead, 6));
    }

    fn reaching(family: Family, tiers: u8) -> SetBonuses {
        let mut sets = SetBonuses::default();
        sets.0[family as usize] = tiers;
        sets
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    fn at(sets: &SetBonuses, stat: Stat, base: f32) -> f32 {
        resolve(stat, base, &set_entries(sets, false))
    }

    #[test]
    fn no_tier_reached_puts_no_entries() {
        assert!(set_entries(&SetBonuses::default(), true).is_empty());
    }

    #[test]
    fn lead_2_adds_half_to_the_headshot_multiplier() {
        assert!(close(at(&reaching(Family::Lead, 1), Stat::HeadshotMultiplier, 3.5), 4.0));
    }

    #[test]
    fn blade_2_adds_a_quarter_to_knife_damage() {
        assert!(close(at(&reaching(Family::Blade, 1), Stat::KnifeDamage, 100.0), 125.0));
    }

    #[test]
    fn spark_2_shortens_ammo_mod_and_trap_cooldowns_by_a_tenth() {
        let sets = reaching(Family::Spark, 1);
        assert!(close(at(&sets, Stat::AmmoModCooldown, 10.0), 9.0));
        assert!(close(at(&sets, Stat::TrapCooldown, 30.0), 27.0));
    }

    #[test]
    fn ember_2_adds_a_fifth_to_equipment_damage_and_burns_tick_faster() {
        let sets = reaching(Family::Ember, 1);
        assert!(close(at(&sets, Stat::EquipmentDamage, 1.0), 1.2));
        assert!(close(at(&sets, Stat::BurnSpeed, 1.0), 1.2));
    }

    #[test]
    fn gold_2_adds_a_tenth_to_the_points_rate() {
        assert!(close(at(&reaching(Family::Gold, 1), Stat::PointsRate, 0.0), 0.1));
    }

    #[test]
    fn grit_2_regenerates_half_again_as_fast_in_a_round() {
        assert!(close(at(&reaching(Family::Grit, 1), Stat::RoundRegen, 2.0), 3.0));
    }

    #[test]
    fn higher_tiers_keep_the_tier_2_stat() {
        assert!(close(at(&reaching(Family::Lead, 3), Stat::HeadshotMultiplier, 3.5), 4.0));
    }

    #[test]
    fn lead_4_adds_half_damage_only_in_the_last_third_of_the_magazine() {
        assert!(magazine_low(10, 30));
        assert!(magazine_low(0, 30));
        assert!(!magazine_low(11, 30));
        assert!(magazine_low(1, 4), "1 of 4 is under a third");
        assert!(!magazine_low(2, 4));
        let damage = |tiers, low| resolve(Stat::Damage, 100.0, &set_entries(&reaching(Family::Lead, tiers), low));
        assert!(close(damage(2, true), 150.0));
        assert!(close(damage(2, false), 100.0));
        assert!(close(damage(1, true), 100.0), "not before Lead 4");
    }

    #[test]
    fn each_tier_is_its_own_source() {
        let mut sets = reaching(Family::Lead, 2);
        sets.0[Family::Ember as usize] = 1;
        for entry in set_entries(&sets, true) {
            assert!(is_set_source(entry.source));
        }
        assert!(!is_set_source(Source("Hollow Points")));
    }

    #[test]
    fn families_count_apart() {
        let mut sets = SetBonuses::default();
        let reached = sets.reach(&holding(&["hollow_points", "trench_raider", "light_feet", "long_arm"]));
        assert_eq!(reached, [(Family::Blade, 2)]);
    }
}
