//! Heat: opt-in difficulty unlocked by the first Win. 8 Heat Conditions, each set to a Degree in
//! the Armory (0 is off), every Degree worth 1 Heat, so Heat runs from 0 to 20. A Condition is
//! its own stat source, so it multiplies with Pact Drawbacks and Twists. Each Degree is the
//! Condition's whole effect at that step, stored as one array per Condition in `tuning`.
//!
//! Most Conditions are plain stat entries on every Player (the Host's Heat is the Room's). The
//! rest are small rules this module answers for the systems that ask: Elite Surge's `EliteRules`,
//! Iron Bosses' health and Boss Round Horde, and Tight Offers' Reroll price and starting Banishes.
//! Classic never has a `RunHeat` above zero.

use bevy::prelude::*;

use super::elite::EliteRules;
use super::progress::{GauntletFile, Refusal};
use crate::modifier::{Effect, Modifier, Source, Stat};
use crate::storage::records::RecordExtra;
use crate::tuning::*;

/// The 8 Heat Conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition {
    Inflation,
    ThickHide,
    SlowMend,
    Restless,
    LeanPickings,
    EliteSurge,
    IronBosses,
    TightOffers,
}

impl Condition {
    pub const ALL: [Condition; 8] = [
        Condition::Inflation,
        Condition::ThickHide,
        Condition::SlowMend,
        Condition::Restless,
        Condition::LeanPickings,
        Condition::EliteSurge,
        Condition::IronBosses,
        Condition::TightOffers,
    ];

    /// The stable ID used in `gauntlet.ron` and Run Records.
    pub fn id(self) -> &'static str {
        match self {
            Condition::Inflation => "inflation",
            Condition::ThickHide => "thick_hide",
            Condition::SlowMend => "slow_mend",
            Condition::Restless => "restless",
            Condition::LeanPickings => "lean_pickings",
            Condition::EliteSurge => "elite_surge",
            Condition::IronBosses => "iron_bosses",
            Condition::TightOffers => "tight_offers",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Condition::Inflation => "Inflation",
            Condition::ThickHide => "Thick Hide",
            Condition::SlowMend => "Slow Mend",
            Condition::Restless => "Restless",
            Condition::LeanPickings => "Lean Pickings",
            Condition::EliteSurge => "Elite Surge",
            Condition::IronBosses => "Iron Bosses",
            Condition::TightOffers => "Tight Offers",
        }
    }

    /// How many Degrees it has.
    pub fn max_degree(self) -> u32 {
        match self {
            Condition::Inflation | Condition::ThickHide | Condition::Restless | Condition::IronBosses => 3,
            Condition::SlowMend | Condition::LeanPickings | Condition::EliteSurge | Condition::TightOffers => 2,
        }
    }

    /// What it does at `degree`, as the Armory shows it.
    pub fn effect(self, degree: u32) -> String {
        let at = (degree as usize).wrapping_sub(1);
        if degree == 0 || degree > self.max_degree() {
            return "Off".to_string();
        }
        let percent = |factor: f32| format!("{:+.0}%", (factor - 1.0) * 100.0);
        match self {
            Condition::Inflation => format!("Prices {}", percent(GAUNTLET_HEAT_INFLATION_PRICE_FACTOR[at])),
            Condition::ThickHide => format!("Zombie health {}", percent(GAUNTLET_HEAT_THICK_HIDE_HEALTH_FACTOR[at])),
            Condition::SlowMend => {
                if GAUNTLET_HEAT_SLOW_MEND_ROUND_REGEN[at] == 0.0 {
                    "Health regen waits twice as long, and none in a Round".to_string()
                } else {
                    "Health regen waits twice as long".to_string()
                }
            }
            Condition::Restless => {
                format!("{:.0}% of Zombies spawn Sprinters", GAUNTLET_HEAT_RESTLESS_SPRINTER_CHANCE[at] * 100.0)
            }
            Condition::LeanPickings => {
                if GAUNTLET_HEAT_LEAN_PICKINGS_MAX_AMMO_IN_BAG[at] < 1.0 {
                    "Half the Power-up drops, and no Max Ammo".to_string()
                } else {
                    "Half the Power-up drops".to_string()
                }
            }
            Condition::EliteSurge => format!(
                "Elites x{} from Round {}, up to {} at once",
                GAUNTLET_HEAT_ELITE_SURGE_CHANCE_FACTOR[at],
                GAUNTLET_HEAT_ELITE_SURGE_FIRST_ROUND[at],
                GAUNTLET_HEAT_ELITE_SURGE_MAX_ALIVE[at]
            ),
            Condition::IronBosses => format!(
                "Boss health {}{}",
                percent(GAUNTLET_HEAT_IRON_BOSSES_HEALTH_FACTOR[at]),
                if GAUNTLET_HEAT_IRON_BOSSES_HORDE_FACTOR[at] >= 1.0 { ", and a full Horde" } else { "" }
            ),
            Condition::TightOffers => format!(
                "Reroll from {} Points{}",
                GAUNTLET_HEAT_TIGHT_OFFERS_REROLL_BASE_PRICE[at],
                if GAUNTLET_HEAT_TIGHT_OFFERS_STARTING_BANISHES[at] == 0 { ", no starting Banish" } else { "" }
            ),
        }
    }

    fn source(self) -> Source {
        Source(match self {
            Condition::Inflation => "Heat: Inflation",
            Condition::ThickHide => "Heat: Thick Hide",
            Condition::SlowMend => "Heat: Slow Mend",
            Condition::Restless => "Heat: Restless",
            Condition::LeanPickings => "Heat: Lean Pickings",
            Condition::EliteSurge => "Heat: Elite Surge",
            Condition::IronBosses => "Heat: Iron Bosses",
            Condition::TightOffers => "Heat: Tight Offers",
        })
    }

    /// The stat entries it puts on every Player at `degree`; the Conditions that are rules
    /// instead (Elite Surge, Iron Bosses, Tight Offers) put none.
    pub fn entries(self, degree: u32) -> Vec<Modifier> {
        if degree == 0 || degree > self.max_degree() {
            return Vec::new();
        }
        let at = degree as usize - 1;
        let entry = |stat, effect| Modifier { stat, source: self.source(), effect };
        match self {
            Condition::Inflation => vec![entry(Stat::Price, Effect::Mult(GAUNTLET_HEAT_INFLATION_PRICE_FACTOR[at]))],
            Condition::ThickHide => vec![entry(Stat::ZombieHealth, Effect::Mult(GAUNTLET_HEAT_THICK_HIDE_HEALTH_FACTOR[at]))],
            Condition::SlowMend => vec![
                entry(Stat::HealthRegenDelay, Effect::Mult(GAUNTLET_HEAT_SLOW_MEND_REGEN_DELAY_FACTOR[at])),
                entry(Stat::RoundRegen, Effect::Mult(GAUNTLET_HEAT_SLOW_MEND_ROUND_REGEN[at])),
            ],
            Condition::Restless => vec![entry(Stat::SprinterChance, Effect::Flat(GAUNTLET_HEAT_RESTLESS_SPRINTER_CHANCE[at]))],
            Condition::LeanPickings => vec![
                entry(Stat::PowerUpDropChance, Effect::Mult(GAUNTLET_HEAT_LEAN_PICKINGS_DROP_FACTOR[at])),
                entry(Stat::MaxAmmoInBag, Effect::Mult(GAUNTLET_HEAT_LEAN_PICKINGS_MAX_AMMO_IN_BAG[at])),
            ],
            Condition::EliteSurge | Condition::IronBosses | Condition::TightOffers => Vec::new(),
        }
    }
}

/// Whether `source` is a Heat Condition's own, so the entries sync leaves everything else alone.
pub fn is_heat_source(source: Source) -> bool {
    Condition::ALL.iter().any(|condition| condition.source() == source)
}

/// The Scrip multiplier a Run at `heat` pays: `1 + GAUNTLET_HEAT_SCRIP_PER_POINT × Heat`.
pub fn scrip_multiplier(heat: u32) -> f64 {
    1.0 + GAUNTLET_HEAT_SCRIP_PER_POINT * f64::from(heat)
}

/// A Run's Scrip total at `heat`, rounded to the nearest whole Scrip. Commendation Scrip is paid
/// separately and never goes through this.
pub fn scaled_scrip(total: u64, heat: u32) -> u64 {
    (total as f64 * scrip_multiplier(heat)).round() as u64
}

/// The Degrees set for the current Run, one per Condition in `Condition::ALL` order. All 0 in
/// Classic and until the first Win.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunHeat(pub [u32; 8]);

impl RunHeat {
    /// The Degrees a Host's Room carries, kept within each Condition's top Degree so that a
    /// bad message can't set one the Run has no row for.
    pub fn from_room(degrees: [u8; 8]) -> RunHeat {
        RunHeat(std::array::from_fn(|at| {
            let condition = Condition::ALL[at];
            u32::from(degrees[at]).min(condition.max_degree())
        }))
    }

    pub fn degree(&self, condition: Condition) -> u32 {
        self.0[condition as usize]
    }

    /// Heat: every Degree is worth 1.
    pub fn total(&self) -> u32 {
        self.0.iter().sum()
    }

    /// The stat entries every Condition puts on each Player.
    pub fn entries(&self) -> Vec<Modifier> {
        Condition::ALL.iter().flat_map(|&condition| condition.entries(self.degree(condition))).collect()
    }

    /// The Elite rules under Elite Surge: the base rules at Degree 0.
    pub fn elite_rules(&self) -> EliteRules {
        match self.degree(Condition::EliteSurge) {
            0 => EliteRules::default(),
            degree => {
                let at = degree as usize - 1;
                EliteRules {
                    chance_factor: f64::from(GAUNTLET_HEAT_ELITE_SURGE_CHANCE_FACTOR[at]),
                    max_chance: f64::from(GAUNTLET_HEAT_ELITE_SURGE_MAX[at]),
                    first_round: GAUNTLET_HEAT_ELITE_SURGE_FIRST_ROUND[at],
                    max_alive: GAUNTLET_HEAT_ELITE_SURGE_MAX_ALIVE[at],
                }
            }
        }
    }

    /// The factor Iron Bosses puts on a Boss's health (the Round's base health × the Act's
    /// multiplier comes first). Thick Hide never reaches a Boss.
    pub fn boss_health_factor(&self) -> f32 {
        match self.degree(Condition::IronBosses) {
            0 => 1.0,
            degree => GAUNTLET_HEAT_IRON_BOSSES_HEALTH_FACTOR[degree as usize - 1],
        }
    }

    /// The Boss Round's `HordeSize` factor.
    pub fn boss_horde_factor(&self) -> f32 {
        match self.degree(Condition::IronBosses) {
            0 => GAUNTLET_BOSS_ROUND_HORDE_FACTOR,
            degree => GAUNTLET_HEAT_IRON_BOSSES_HORDE_FACTOR[degree as usize - 1],
        }
    }

    /// A Reroll's base price under Tight Offers.
    pub fn reroll_base_price(&self) -> u64 {
        match self.degree(Condition::TightOffers) {
            0 => GAUNTLET_REROLL_BASE_PRICE,
            degree => GAUNTLET_HEAT_TIGHT_OFFERS_REROLL_BASE_PRICE[degree as usize - 1],
        }
    }

    /// The base starting Banishes under Tight Offers.
    pub fn starting_banishes(&self) -> u32 {
        match self.degree(Condition::TightOffers) {
            0 => GAUNTLET_BANISHES_START,
            degree => GAUNTLET_HEAT_TIGHT_OFFERS_STARTING_BANISHES[degree as usize - 1],
        }
    }

    /// The Conditions above 0, for a Run Record.
    pub fn record_conditions(&self) -> Vec<RecordExtra> {
        Condition::ALL
            .iter()
            .filter(|&&condition| self.degree(condition) > 0)
            .map(|&condition| RecordExtra { id: condition.id().to_string(), count: self.degree(condition) })
            .collect()
    }

    /// "Heat 7 · Scrip ×1.7".
    pub fn banner(&self) -> String {
        format!("Heat {} · Scrip ×{:.1}", self.total(), scrip_multiplier(self.total()))
    }
}

/// A Condition's name from its stable ID, or the ID itself for one this build doesn't know.
pub fn condition_name(id: &str) -> String {
    Condition::ALL
        .iter()
        .find(|condition| condition.id() == id)
        .map_or_else(|| id.to_string(), |condition| condition.name().to_string())
}

/// "Conditions: Inflation 2, Thick Hide 1" for the Game Over screen and the Record detail.
pub fn conditions_line(heat: &RunHeat) -> String {
    let set: Vec<String> = Condition::ALL
        .iter()
        .filter(|&&condition| heat.degree(condition) > 0)
        .map(|&condition| format!("{} {}", condition.name(), heat.degree(condition)))
        .collect();
    format!("Conditions: {}", set.join(", "))
}

impl GauntletFile {
    /// The Degree set for `condition`, never past its top.
    pub fn heat_degree(&self, condition: Condition) -> u32 {
        self.heat
            .iter()
            .find(|(id, _)| id == condition.id())
            .map_or(0, |&(_, degree)| degree.min(condition.max_degree()))
    }

    /// The Degrees a Run starts at: whatever's set once Heat is unlocked, nothing before.
    pub fn run_heat(&self) -> RunHeat {
        let mut degrees = [0; 8];
        if self.has_won {
            for condition in Condition::ALL {
                degrees[condition as usize] = self.heat_degree(condition);
            }
        }
        RunHeat(degrees)
    }

    /// Raises `condition` one Degree, wrapping back to 0 past its top. Heat is locked until the
    /// first Win.
    pub fn cycle_heat(&mut self, condition: Condition) -> Result<(), Refusal> {
        if !self.has_won {
            return Err(Refusal::HeatLocked);
        }
        let next = (self.heat_degree(condition) + 1) % (condition.max_degree() + 1);
        self.heat.retain(|(id, _)| id != condition.id());
        if next > 0 {
            self.heat.push((condition.id().to_string(), next));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modifier::resolve;

    fn heat(set: &[(Condition, u32)]) -> RunHeat {
        let mut degrees = [0; 8];
        for &(condition, degree) in set {
            degrees[condition as usize] = degree;
        }
        RunHeat(degrees)
    }

    fn maxed() -> RunHeat {
        RunHeat(Condition::ALL.map(Condition::max_degree))
    }

    #[test]
    fn heat_is_the_sum_of_the_degrees_and_tops_out_at_twenty() {
        assert_eq!(RunHeat::default().total(), 0);
        assert_eq!(heat(&[(Condition::Inflation, 2), (Condition::IronBosses, 3)]).total(), 5);
        assert_eq!(maxed().total(), 20);
    }

    #[test]
    fn each_degree_is_the_conditions_whole_effect_at_that_step() {
        for (at, degree) in (1..=3).enumerate() {
            let price = |c: Condition| resolve(Stat::Price, 100.0, &c.entries(degree));
            assert!((price(Condition::Inflation) - 100.0 * GAUNTLET_HEAT_INFLATION_PRICE_FACTOR[at]).abs() < 1e-3);
            let health = resolve(Stat::ZombieHealth, 1.0, &Condition::ThickHide.entries(degree));
            assert!((health - GAUNTLET_HEAT_THICK_HIDE_HEALTH_FACTOR[at]).abs() < 1e-6);
            let sprint = resolve(Stat::SprinterChance, 0.0, &Condition::Restless.entries(degree));
            assert!((sprint - GAUNTLET_HEAT_RESTLESS_SPRINTER_CHANCE[at]).abs() < 1e-6);
        }
        assert_eq!(resolve(Stat::Price, 100.0, &Condition::Inflation.entries(0)), 100.0, "off is neutral");
        assert!(Condition::Inflation.entries(4).is_empty(), "past the top is nothing");
    }

    #[test]
    fn slow_mend_doubles_the_regen_delay_and_stops_round_regen_at_degree_two() {
        for degree in [1, 2] {
            assert_eq!(resolve(Stat::HealthRegenDelay, 5.0, &Condition::SlowMend.entries(degree)), 10.0);
        }
        assert_eq!(resolve(Stat::RoundRegen, 1.0, &Condition::SlowMend.entries(1)), 1.0);
        assert_eq!(resolve(Stat::RoundRegen, 1.0, &Condition::SlowMend.entries(2)), 0.0);
    }

    #[test]
    fn lean_pickings_halves_drops_and_degree_two_leaves_max_ammo_out_of_the_bag() {
        for degree in [1, 2] {
            assert_eq!(resolve(Stat::PowerUpDropChance, 1.0, &Condition::LeanPickings.entries(degree)), 0.5);
        }
        assert!(resolve(Stat::MaxAmmoInBag, 1.0, &Condition::LeanPickings.entries(1)) >= 1.0);
        assert!(resolve(Stat::MaxAmmoInBag, 1.0, &Condition::LeanPickings.entries(2)) < 1.0);
    }

    #[test]
    fn a_condition_is_its_own_source_so_it_multiplies_with_other_sources() {
        let twist = Modifier { stat: Stat::Price, source: Source("Twist"), effect: Effect::Mult(1.5) };
        let mut entries = Condition::Inflation.entries(1);
        entries.push(twist);
        assert!((resolve(Stat::Price, 100.0, &entries) - 100.0 * 1.10 * 1.5).abs() < 1e-3);
        assert!(entries.iter().all(|entry| entry.source == Condition::Inflation.source() || entry.source == Source("Twist")));
        assert!(is_heat_source(Condition::Inflation.source()) && !is_heat_source(Source("Twist")));
    }

    #[test]
    fn the_scrip_multiplier_is_one_plus_a_tenth_per_heat() {
        assert_eq!(scrip_multiplier(0), 1.0);
        assert!((scrip_multiplier(7) - 1.7).abs() < 1e-12);
        assert_eq!(scaled_scrip(785, 0), 785);
        assert_eq!(scaled_scrip(785, 10), 1570);
        assert_eq!(scaled_scrip(0, 20), 0);
    }

    #[test]
    fn elite_surge_changes_the_elite_rules_by_degree() {
        assert_eq!(heat(&[]).elite_rules(), EliteRules::default());
        let one = heat(&[(Condition::EliteSurge, 1)]).elite_rules();
        assert_eq!((one.first_round, one.max_alive), (6, 3));
        assert!((one.chance_factor - 1.5).abs() < 1e-6 && (one.max_chance - 0.225).abs() < 1e-6);
        let two = heat(&[(Condition::EliteSurge, 2)]).elite_rules();
        assert_eq!((two.first_round, two.max_alive), (3, 5));
        assert!((two.chance_factor - 2.0).abs() < 1e-6 && (two.max_chance - 0.30).abs() < 1e-6);
    }

    #[test]
    fn iron_bosses_scales_boss_health_and_degree_three_restores_the_full_horde() {
        assert_eq!(heat(&[]).boss_health_factor(), 1.0);
        assert_eq!(heat(&[]).boss_horde_factor(), GAUNTLET_BOSS_ROUND_HORDE_FACTOR);
        let at = |degree| heat(&[(Condition::IronBosses, degree)]);
        assert_eq!([1, 2, 3].map(|d| at(d).boss_health_factor()), [1.25, 1.5, 1.5]);
        assert_eq!([1, 2, 3].map(|d| at(d).boss_horde_factor()), [0.5, 0.5, 1.0]);
    }

    #[test]
    fn thick_hide_never_stands_in_for_iron_bosses() {
        let thick = heat(&[(Condition::ThickHide, 3)]);
        assert_eq!(thick.boss_health_factor(), 1.0);
    }

    #[test]
    fn tight_offers_raises_the_reroll_base_and_degree_two_drops_the_starting_banish() {
        assert_eq!((heat(&[]).reroll_base_price(), heat(&[]).starting_banishes()), (1000, 1));
        let at = |degree| heat(&[(Condition::TightOffers, degree)]);
        assert_eq!((at(1).reroll_base_price(), at(1).starting_banishes()), (2000, 1));
        assert_eq!((at(2).reroll_base_price(), at(2).starting_banishes()), (2000, 0));
    }

    #[test]
    fn heat_stays_locked_until_the_first_win() {
        let mut file = GauntletFile::default();
        assert_eq!(file.cycle_heat(Condition::Inflation), Err(Refusal::HeatLocked));
        file.heat = vec![("inflation".into(), 2)];
        assert_eq!(file.run_heat().total(), 0, "a saved Degree doesn't apply before the Win");
        file.has_won = true;
        assert_eq!(file.run_heat().degree(Condition::Inflation), 2);
    }

    #[test]
    fn clicking_a_condition_raises_its_degree_and_wraps_back_to_zero() {
        let mut file = GauntletFile { has_won: true, ..default() };
        let seen: Vec<u32> = (0..4)
            .map(|_| {
                file.cycle_heat(Condition::SlowMend).unwrap();
                file.heat_degree(Condition::SlowMend)
            })
            .collect();
        assert_eq!(seen, [1, 2, 0, 1]);
        file.cycle_heat(Condition::SlowMend).unwrap();
        file.cycle_heat(Condition::SlowMend).unwrap();
        assert!(file.heat.is_empty(), "Conditions at 0 aren't listed");
    }

    #[test]
    fn a_saved_degree_past_the_top_is_clamped() {
        let file = GauntletFile { has_won: true, heat: vec![("slow_mend".into(), 9), ("nonsense".into(), 3)], ..default() };
        assert_eq!(file.run_heat().degree(Condition::SlowMend), 2);
        assert_eq!(file.run_heat().total(), 2);
    }

    #[test]
    fn the_record_lists_only_the_conditions_above_zero() {
        let record = heat(&[(Condition::Restless, 3), (Condition::TightOffers, 1)]).record_conditions();
        assert_eq!(
            record,
            [RecordExtra { id: "restless".into(), count: 3 }, RecordExtra { id: "tight_offers".into(), count: 1 }]
        );
        assert!(RunHeat::default().record_conditions().is_empty());
    }

    #[test]
    fn the_banner_shows_heat_and_the_scrip_multiplier() {
        assert_eq!(heat(&[(Condition::Inflation, 3), (Condition::ThickHide, 3), (Condition::Restless, 1)]).banner(), "Heat 7 · Scrip ×1.7");
    }
}
