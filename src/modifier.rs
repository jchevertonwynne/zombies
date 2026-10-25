//! The stat-modifier seam. Perks, Pack-a-Punch, Attachments and anything layered on top add
//! entries; every stat that can change is re-resolved from its base and the current entries each
//! time it's read, so nothing goes stale when entries come and go.

use bevy::prelude::*;

/// A stat that modifier entries can change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stat {
    /// Damage per bullet.
    Damage,
    /// Seconds between shots.
    FireInterval,
    /// Seconds a Reload takes.
    ReloadTime,
    /// Bullets a full Magazine holds.
    MagazineSize,
    /// The Player's walk and sprint speed.
    MoveSpeed,
}

/// Where an entry comes from. Each Perk, Pack-a-Punch, Attachment and so on is its own source:
/// multipliers from one source add together, multipliers from different sources multiply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Source(pub &'static str);

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(
    dead_code,
    reason = "the seam has no entries until Perks and Pack-a-Punch arrive"
)]
pub enum Effect {
    /// Added to the base before any multiplier.
    Flat(f32),
    /// A multiplier such as ×0.8. Within one source these add (two ×1.1 make ×1.2).
    Mult(f32),
    /// `r` applied `n` times: ×rⁿ.
    Reduction(f32, u32),
    /// A chance `c` rolled `n` times, combined hyperbolically: `1 − (1 − c)ⁿ`.
    Chance(f32, u32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Modifier {
    pub stat: Stat,
    pub source: Source,
    pub effect: Effect,
}

/// Modifier entries held by the Player. A Weapon copy carries its own.
#[derive(Component, Default)]
pub struct Modifiers(pub Vec<Modifier>);

/// One stat's value: `(base + Σflat) × Π(one multiplier per source) × Π rⁿ`, then any chances
/// folded in hyperbolically, `1 − (1 − value) × Π(1 − c)ⁿ`.
pub fn resolve<'a>(stat: Stat, base: f32, entries: impl IntoIterator<Item = &'a Modifier>) -> f32 {
    let mut flat = 0.0;
    let mut reduction = 1.0;
    let mut miss = 1.0;
    // Each source's multiplier, kept as the sum of its deltas from ×1.
    let mut sources: Vec<(Source, f32)> = Vec::new();
    for entry in entries.into_iter().filter(|entry| entry.stat == stat) {
        match entry.effect {
            Effect::Flat(x) => flat += x,
            Effect::Mult(x) => match sources
                .iter_mut()
                .find(|(source, _)| *source == entry.source)
            {
                Some((_, delta)) => *delta += x - 1.0,
                None => sources.push((entry.source, x - 1.0)),
            },
            Effect::Reduction(r, n) => reduction *= r.powi(n as i32),
            Effect::Chance(c, n) => miss *= (1.0 - c).powi(n as i32),
        }
    }
    let multiplier: f32 = sources.iter().map(|(_, delta)| 1.0 + delta).product();
    let value = (base + flat) * multiplier * reduction;
    if miss < 1.0 {
        1.0 - (1.0 - value) * miss
    } else {
        value
    }
}

/// A resolved count, such as a Magazine size, rounded up.
pub fn resolve_count<'a>(
    stat: Stat,
    base: u32,
    entries: impl IntoIterator<Item = &'a Modifier>,
) -> u32 {
    // The small nudge keeps float noise (12.000001) from rounding up a whole bullet.
    (resolve(stat, base as f32, entries) - 1e-4).ceil().max(0.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(stat: Stat, source: &'static str, effect: Effect) -> Modifier {
        Modifier {
            stat,
            source: Source(source),
            effect,
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn no_entries_leave_the_base_alone() {
        assert_eq!(resolve(Stat::Damage, 20.0, &[]), 20.0);
    }

    #[test]
    fn multipliers_from_different_sources_multiply() {
        // Rapid Fire ×0.8 with Double Tap ×0.75 gives ×0.6.
        let entries = [
            entry(Stat::FireInterval, "Rapid Fire", Effect::Mult(0.8)),
            entry(Stat::FireInterval, "Double Tap", Effect::Mult(0.75)),
        ];
        assert!(close(resolve(Stat::FireInterval, 1.0, &entries), 0.6));
    }

    #[test]
    fn multipliers_within_one_source_add() {
        // Three Stacks of +10% make +30%, not 1.1³.
        let entries = [entry(Stat::Damage, "Boon", Effect::Mult(1.1)); 3];
        assert!(close(resolve(Stat::Damage, 100.0, &entries), 130.0));
    }

    #[test]
    fn stacked_sources_then_multiply_with_each_other() {
        let entries = [
            entry(Stat::Damage, "A", Effect::Mult(3.0)),
            entry(Stat::Damage, "B", Effect::Mult(1.15)),
            entry(Stat::Damage, "B", Effect::Mult(1.15)),
        ];
        assert!(close(resolve(Stat::Damage, 1.0, &entries), 3.0 * 1.3));
    }

    #[test]
    fn flats_add_before_multipliers() {
        let entries = [
            entry(Stat::Damage, "A", Effect::Flat(10.0)),
            entry(Stat::Damage, "B", Effect::Mult(2.0)),
        ];
        assert!(close(resolve(Stat::Damage, 20.0, &entries), 60.0));
    }

    #[test]
    fn reductions_apply_once_per_count() {
        let entries = [entry(Stat::ReloadTime, "A", Effect::Reduction(0.9, 2))];
        assert!(close(resolve(Stat::ReloadTime, 1.0, &entries), 0.81));
    }

    #[test]
    fn chances_are_hyperbolic() {
        let one = [entry(Stat::Damage, "A", Effect::Chance(0.1, 3))];
        assert!(close(
            resolve(Stat::Damage, 0.0, &one),
            1.0 - 0.9_f32.powi(3)
        ));

        // Two sources of 50% make 75%, never 100%.
        let two = [
            entry(Stat::Damage, "A", Effect::Chance(0.5, 1)),
            entry(Stat::Damage, "B", Effect::Chance(0.5, 1)),
        ];
        assert!(close(resolve(Stat::Damage, 0.0, &two), 0.75));
    }

    #[test]
    fn entries_for_other_stats_are_ignored() {
        let entries = [entry(Stat::MoveSpeed, "A", Effect::Mult(2.0))];
        assert_eq!(resolve(Stat::Damage, 20.0, &entries), 20.0);
    }

    #[test]
    fn counts_round_up() {
        let entries = [entry(Stat::MagazineSize, "Extended Mag", Effect::Mult(1.5))];
        assert_eq!(resolve_count(Stat::MagazineSize, 8, &entries), 12);
        assert_eq!(resolve_count(Stat::MagazineSize, 15, &entries), 23);
    }
}
