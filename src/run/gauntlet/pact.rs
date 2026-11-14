//! Pacts: the table every Pact is a row of, and the pure rules for the Pact Offer. A Pact is a
//! strong fixed upside with a Drawback that lasts the Run, both as stat-modifier entries where
//! Classic has a stat for them; Scorched Earth and Overkill also have a small system of their own.

use bevy::prelude::*;

use super::boon::{Requirement, RunFacts, draw};
use crate::weapons::ammo_mod::AmmoMods;
use crate::weapons::arsenal::Splash;
use crate::weapons::explosive::Explosion;
use crate::modifier::{Effect, Modifier, Source, Stat};
use crate::economy::perk::Perk;
use crate::players::player::Seat;
use crate::run::round::alive_cap;
use crate::tuning::*;

/// Pact cards and held Pacts are crimson.
pub const CRIMSON: Color = Color::srgb(0.86, 0.1, 0.22);

/// A Pact by stable ID and display name. Boons' Requirements name Pacts this way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PactRef {
    /// The stable ID used in save files and Run Records. The display name can change.
    pub id: &'static str,
    pub name: &'static str,
}

pub const GLASS_JAW: PactRef = PactRef {
    id: "glass_jaw",
    name: "Glass Jaw",
};
pub const GOLD_RUSH: PactRef = PactRef {
    id: "gold_rush",
    name: "Gold Rush",
};
pub const SCORCHED_EARTH: PactRef = PactRef {
    id: "scorched_earth",
    name: "Scorched Earth",
};
pub const OPEN_BAR: PactRef = PactRef {
    id: "open_bar",
    name: "Open Bar",
};
pub const UNDYING: PactRef = PactRef {
    id: "undying",
    name: "Undying",
};
pub const ARSENAL: PactRef = PactRef {
    id: "arsenal",
    name: "Arsenal",
};
pub const OVERCLOCKED: PactRef = PactRef {
    id: "overclocked",
    name: "Overclocked",
};
pub const COLD_STEEL: PactRef = PactRef {
    id: "cold_steel",
    name: "Cold Steel",
};
pub const ENDLESS_BELT: PactRef = PactRef {
    id: "endless_belt",
    name: "Endless Belt",
};
pub const LIGHTNING_ROD: PactRef = PactRef {
    id: "lightning_rod",
    name: "Lightning Rod",
};
pub const OVERKILL: PactRef = PactRef {
    id: "overkill",
    name: "Overkill",
};
pub const DEAD_RUN: PactRef = PactRef {
    id: "dead_run",
    name: "Dead Run",
};

pub struct PactDef {
    pub pact: PactRef,
    /// All must hold when a Pact Offer is drawn.
    pub requirements: &'static [Requirement],
    /// Its stat-modifier entries, upside and Drawback alike. The Pact is their one source.
    pub entries: &'static [(Stat, Effect)],
    /// The stats among `entries` that are world Drawbacks: they change the world (prices, the
    /// Horde, Zombies, drops, Max Ammo) and fall on every Player, whoever took the Pact.
    pub world_drawbacks: &'static [Stat],
    pub upside: fn() -> String,
    /// Its Drawback text, with real numbers for the given Round.
    pub drawback: fn(round: u32) -> String,
}

impl PactDef {
    pub fn id(&self) -> &'static str {
        self.pact.id
    }

    pub fn name(&self) -> &'static str {
        self.pact.name
    }
}

/// A whole percentage from a factor: 1.25 → "25".
fn percent_over(factor: f32) -> String {
    format!("{:.0}", (f64::from(factor) - 1.0) * 100.0)
}

/// Every Pact.
pub const PACTS: &[PactDef] = &[
    PactDef {
        pact: GLASS_JAW,
        requirements: &[],
        world_drawbacks: &[],
        entries: &[
            (Stat::Damage, Effect::Mult(PACT_GLASS_JAW_DAMAGE_FACTOR)),
            (
                Stat::MaxHealth,
                Effect::Flat(-PACT_GLASS_JAW_MAX_HEALTH_LOSS),
            ),
        ],
        upside: || {
            format!(
                "Weapon damage +{}%",
                percent_over(PACT_GLASS_JAW_DAMAGE_FACTOR)
            )
        },
        drawback: |_| {
            format!(
                "max Health -{:.0} (one swipe Downs without Juggernog)",
                PACT_GLASS_JAW_MAX_HEALTH_LOSS
            )
        },
    },
    PactDef {
        pact: GOLD_RUSH,
        requirements: &[],
        world_drawbacks: &[Stat::Price],
        entries: &[
            (
                Stat::PointsRate,
                Effect::Flat(PACT_GOLD_RUSH_POINTS_FACTOR - 1.0),
            ),
            (Stat::Price, Effect::Mult(PACT_GOLD_RUSH_PRICE_FACTOR)),
        ],
        upside: || {
            let factor = PACT_GOLD_RUSH_POINTS_FACTOR;
            format!(
                "all Points x{factor:.0}, on top of Double Points (x{:.0} during it)",
                factor + 1.0
            )
        },
        drawback: |_| format!("every price x{PACT_GOLD_RUSH_PRICE_FACTOR:.2}"),
    },
    PactDef {
        pact: SCORCHED_EARTH,
        requirements: &[],
        world_drawbacks: &[Stat::ZombieSpeed],
        entries: &[(
            Stat::ZombieSpeed,
            Effect::Mult(PACT_SCORCHED_EARTH_ZOMBIE_SPEED_FACTOR),
        )],
        upside: || {
            format!(
                "every Zombie killed explodes for {:.0}% of its max Health within {:.0} m \
                 (never hurts you; can make Crawlers)",
                f64::from(PACT_SCORCHED_EARTH_HEALTH_FRACTION) * 100.0,
                PACT_SCORCHED_EARTH_RADIUS_M
            )
        },
        drawback: |_| {
            format!(
                "Zombies move +{}% faster",
                percent_over(PACT_SCORCHED_EARTH_ZOMBIE_SPEED_FACTOR)
            )
        },
    },
    PactDef {
        pact: OPEN_BAR,
        requirements: &[Requirement::PowerOn],
        world_drawbacks: &[Stat::HordeSize],
        entries: &[
            (Stat::PerkPrice, Effect::Mult(0.0)),
            (Stat::PerksKeptOnDown, Effect::Flat(Perk::ALL.len() as f32)),
            (Stat::HordeSize, Effect::Mult(PACT_OPEN_BAR_HORDE_FACTOR)),
        ],
        upside: || "Perks are free and survive a Down".into(),
        drawback: |_| format!("Horde +{}%", percent_over(PACT_OPEN_BAR_HORDE_FACTOR)),
    },
    PactDef {
        pact: UNDYING,
        requirements: &[],
        world_drawbacks: &[],
        entries: &[
            (
                Stat::SelfRevives,
                Effect::Flat(PACT_UNDYING_SELF_REVIVES as f32),
            ),
            (Stat::RoundRegen, Effect::Mult(0.0)),
        ],
        upside: || {
            format!(
                "{PACT_UNDYING_SELF_REVIVES} Self-Revives without Quick Revive (stacks with it)"
            )
        },
        drawback: |_| "Health only regenerates in the Intermission".into(),
    },
    PactDef {
        pact: ARSENAL,
        requirements: &[Requirement::PowerOn],
        world_drawbacks: &[Stat::BoxPrice, Stat::TeddyChance],
        entries: &[
            (Stat::BoxUpgradeChance, Effect::Chance(1.0, 1)),
            (Stat::BoxPrice, Effect::Mult(PACT_ARSENAL_BOX_PRICE_FACTOR)),
            (
                Stat::TeddyChance,
                Effect::Mult(PACT_ARSENAL_TEDDY_CHANCE_FACTOR),
            ),
        ],
        upside: || "every Box Weapon comes out Upgraded".into(),
        drawback: |_| {
            format!(
                "Box price x{PACT_ARSENAL_BOX_PRICE_FACTOR:.0}; Teddy chance x{PACT_ARSENAL_TEDDY_CHANCE_FACTOR:.0}"
            )
        },
    },
    PactDef {
        pact: OVERCLOCKED,
        requirements: &[Requirement::HoldsAmmoMod],
        world_drawbacks: &[Stat::PackAPunchPrice],
        entries: &[
            (Stat::AmmoModCooldown, Effect::Mult(0.0)),
            (
                Stat::PackAPunchPrice,
                Effect::Mult(PACT_OVERCLOCKED_PRICE_FACTOR),
            ),
        ],
        upside: || "Ammo Mods and Procs have no cooldown".into(),
        drawback: |_| format!("Pack-a-Punch and Repack prices x{PACT_OVERCLOCKED_PRICE_FACTOR:.0}"),
    },
    PactDef {
        pact: COLD_STEEL,
        requirements: &[],
        world_drawbacks: &[],
        entries: &[
            (Stat::KnifeKillChance, Effect::Chance(1.0, 1)),
            (
                Stat::ReserveSize,
                Effect::Mult(PACT_COLD_STEEL_RESERVE_FACTOR),
            ),
        ],
        upside: || "the Knife always kills".into(),
        drawback: |_| {
            format!(
                "Reserve Ammo capacity -{:.0}%",
                (1.0 - f64::from(PACT_COLD_STEEL_RESERVE_FACTOR)) * 100.0
            )
        },
    },
    PactDef {
        pact: ENDLESS_BELT,
        requirements: &[],
        world_drawbacks: &[Stat::MaxAmmoInBag],
        entries: &[
            (Stat::ReserveFeed, Effect::Flat(1.0)),
            (Stat::MaxAmmoInBag, Effect::Mult(0.0)),
        ],
        upside: || "no Reloads: Weapons fire straight from Reserve Ammo".into(),
        drawback: |_| "Max Ammo leaves the Power-up bag".into(),
    },
    PactDef {
        pact: LIGHTNING_ROD,
        requirements: &[Requirement::PowerOn],
        world_drawbacks: &[Stat::PowerUpDropChance],
        entries: &[
            (Stat::TrapPrice, Effect::Mult(0.0)),
            (Stat::TrapCooldown, Effect::Mult(0.0)),
            (
                Stat::PowerUpDropChance,
                Effect::Mult(PACT_LIGHTNING_ROD_DROP_CHANCE_FACTOR),
            ),
        ],
        upside: || "Traps are free, with no cooldown".into(),
        drawback: |_| "Power-up drop chance halved".into(),
    },
    PactDef {
        pact: OVERKILL,
        requirements: &[],
        world_drawbacks: &[Stat::AliveCap],
        entries: &[(
            Stat::AliveCap,
            Effect::Flat(PACT_OVERKILL_ALIVE_CAP_BONUS as f32),
        )],
        upside: || {
            format!(
                "damage beyond a kill carries over to the nearest Zombie within {:.0} m",
                PACT_OVERKILL_CARRY_RADIUS_M
            )
        },
        drawback: |round| {
            let cap = alive_cap(round, ALIVE_CAP);
            format!("Alive Cap {cap} -> {}", cap + PACT_OVERKILL_ALIVE_CAP_BONUS)
        },
    },
    PactDef {
        pact: DEAD_RUN,
        requirements: &[],
        world_drawbacks: &[Stat::SprinterChance],
        entries: &[
            (
                Stat::MoveSpeed,
                Effect::Mult(PACT_DEAD_RUN_MOVE_SPEED_FACTOR),
            ),
            (Stat::SprintDrain, Effect::Mult(0.0)),
            (Stat::SprinterChance, Effect::Chance(1.0, 1)),
        ],
        upside: || {
            format!(
                "+{}% move speed and unlimited sprint",
                percent_over(PACT_DEAD_RUN_MOVE_SPEED_FACTOR)
            )
        },
        drawback: |_| "every Zombie is a Sprinter from Round 1".into(),
    },
];

/// A Pact and the Seat of the Player who took it.
#[derive(Clone, Copy)]
pub struct HeldPact {
    pub pact: &'static PactDef,
    pub holder: Seat,
}

/// The Pacts held this Run, in pick order, each with its holder. Each at most once; never removed,
/// and kept through a Down. It's the Run's, not a Player's: a Pact's Drawback lands on everyone.
#[derive(Resource, Default)]
pub struct HeldPacts(pub Vec<HeldPact>);

impl HeldPacts {
    pub fn holds(&self, id: &str) -> bool {
        self.0.iter().any(|held| held.pact.id() == id)
    }

    /// `holder` takes a Pact. One already held stays held once.
    pub fn add(&mut self, pact: &'static PactDef, holder: Seat) {
        if !self.holds(pact.id()) {
            self.0.push(HeldPact { pact, holder });
        }
    }

    /// Who holds the Pact `id`, if anyone does.
    pub fn holder(&self, id: &str) -> Option<Seat> {
        self.0.iter().find(|held| held.pact.id() == id).map(|held| held.holder)
    }

    /// The stable IDs held, for Requirements.
    pub fn ids(&self) -> Vec<&'static str> {
        self.0.iter().map(|held| held.pact.id()).collect()
    }

    /// The stat-modifier entries the held Pacts put on the Player in `seat`: the upsides of the
    /// Pacts they hold, and every Pact's world Drawbacks. Each Pact is its own source.
    pub fn modifiers(&self, seat: Seat) -> Vec<Modifier> {
        self.0
            .iter()
            .flat_map(|held| {
                let mine = held.holder == seat;
                held.pact
                    .entries
                    .iter()
                    .filter(move |&&(stat, _)| mine || held.pact.world_drawbacks.contains(&stat))
                    .map(|&(stat, effect)| Modifier {
                        stat,
                        source: Source(held.pact.name()),
                        effect,
                    })
            })
            .collect()
    }
}

/// Whether a stat-modifier entry comes from a Pact.
pub fn is_pact_source(source: Source) -> bool {
    PACTS.iter().any(|pact| Source(pact.name()) == source)
}


/// Whether a Pact can be on a Pact Offer: Unlocked, not held, with its Requirement holding now.
pub fn pact_eligible(pact: &PactDef, held: &HeldPacts, run: &RunFacts) -> bool {
    // Every Pact is bought in the Armory.
    (run.unlock_all || run.unlocked.iter().any(|id| id == pact.id()))
        && !held.holds(pact.id())
        && pact
            .requirements
            .iter()
            .all(|requirement| requirement.holds(run))
}

/// A Pact Offer: up to `PACT_OFFER_SIZE` eligible Pacts from `table`, drawn uniformly, leaving out
/// any in `claimed` (on a teammate's open Offer). Empty means a Boon Offer is made instead.
pub fn draw_pact_offer(
    table: &'static [PactDef],
    held: &HeldPacts,
    claimed: &[&str],
    run: &RunFacts,
    roll: impl FnMut(f64) -> f64,
) -> Vec<&'static PactDef> {
    let pool = table
        .iter()
        .filter(|pact| pact_eligible(pact, held, run) && !claimed.contains(&pact.id()))
        .map(|pact| (pact, 1.0))
        .collect();
    draw(pool, PACT_OFFER_SIZE, roll)
}

/// Scorched Earth: the blast a killed Zombie goes off with, from its feet at `at`. It never hurts
/// the Player and can make Crawlers. Its kills credit `player`, whose kill set it off.
pub fn scorched_earth_blast(at: Vec3, max_health: f32, player: Option<Entity>) -> Explosion {
    let damage = max_health * PACT_SCORCHED_EARTH_HEALTH_FRACTION;
    Explosion {
        at: at + Vec3::Y * PACT_SCORCHED_EARTH_BLAST_HEIGHT_M,
        thrower: player,
        splash: Splash {
            max: damage,
            min: damage,
            radius: PACT_SCORCHED_EARTH_RADIUS_M,
        },
        hurts_player: false,
        ammo_mods: AmmoMods::NONE,
        crawlers: true,
    }
}

/// Overkill: the Zombie a kill's `overflow` damage carries to, the nearest of `zombies` within
/// `PACT_OVERKILL_CARRY_RADIUS_M` of where it died (measured across the floor), if there's any
/// damage to carry.
pub fn overkill_target<T: Copy>(
    at: Vec3,
    overflow: f32,
    zombies: impl IntoIterator<Item = (T, Vec3)>,
) -> Option<T> {
    if overflow <= 0.0 {
        return None;
    }
    zombies
        .into_iter()
        .map(|(zombie, centre)| (zombie, (centre - at).with_y(0.0).length()))
        .filter(|&(_, distance)| distance <= PACT_OVERKILL_CARRY_RADIUS_M)
        .min_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(zombie, _)| zombie)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::buy::charged;
    use crate::run::gauntlet::boon::{BOONS, HeldBoons, eligible};
    use crate::modifier::{resolve, resolve_count, resolve_points};

    fn pact(r: PactRef) -> &'static PactDef {
        PACTS.iter().find(|pact| pact.pact == r).unwrap()
    }

    /// The Player's entries holding one Pact.
    fn holding(r: PactRef) -> Vec<Modifier> {
        let mut held = HeldPacts::default();
        held.add(pact(r), Seat(0));
        held.modifiers(Seat(0))
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    fn unlocked(power_on: bool) -> RunFacts {
        RunFacts {
            power_on,
            unlock_all: true,
            ..default()
        }
    }

    #[test]
    fn a_pact_offer_draws_three_distinct_unheld_pacts() {
        let run = RunFacts {
            holds_ammo_mod: true,
            ..unlocked(true)
        };
        let offer = draw_pact_offer(PACTS, &HeldPacts::default(), &[], &run, |total| total * 0.5);
        assert_eq!(offer.len(), PACT_OFFER_SIZE);
        assert_ne!(offer[0].id(), offer[1].id());
        assert_ne!(offer[1].id(), offer[2].id());
        assert_ne!(offer[0].id(), offer[2].id());
    }

    #[test]
    fn pacts_are_drawn_uniformly() {
        let mut first_total = None;
        draw_pact_offer(PACTS, &HeldPacts::default(), &[], &unlocked(false), |total| {
            first_total.get_or_insert(total);
            0.0
        });
        // Every eligible Pact weighs 1: the eight without a Requirement, Power off and no Ammo Mod.
        assert_eq!(first_total, Some(8.0));
    }

    #[test]
    fn a_held_pact_never_comes_back() {
        let mut held = HeldPacts::default();
        held.add(pact(COLD_STEEL), Seat(0));
        held.add(pact(COLD_STEEL), Seat(0));
        assert_eq!(held.0.len(), 1, "each Pact is held once");
        let run = unlocked(true);
        for _ in 0..20 {
            let offer = draw_pact_offer(PACTS, &held, &[], &run, |total| rand::random_range(0.0..total));
            assert!(offer.iter().all(|pact| pact.id() != COLD_STEEL.id));
        }
    }

    #[test]
    fn locked_or_unmet_pacts_are_left_out() {
        let held = HeldPacts::default();
        // With nothing bought in the Armory, no Pact is Unlocked.
        assert!(draw_pact_offer(PACTS, &held, &[], &RunFacts::default(), |_| 0.0).is_empty());
        let open_bar = pact(OPEN_BAR);
        assert!(!pact_eligible(open_bar, &held, &unlocked(false)));
        let bought = RunFacts {
            power_on: true,
            unlocked: vec![OPEN_BAR.id.into()],
            ..default()
        };
        assert!(pact_eligible(open_bar, &held, &bought));
        assert!(!pact_eligible(pact(GLASS_JAW), &held, &bought));
        assert!(pact_eligible(open_bar, &held, &unlocked(true)));
        let overclocked = pact(OVERCLOCKED);
        assert!(!pact_eligible(overclocked, &held, &unlocked(true)));
        let with_mod = RunFacts {
            holds_ammo_mod: true,
            ..unlocked(true)
        };
        assert!(pact_eligible(overclocked, &held, &with_mod));
    }

    #[test]
    fn with_every_pact_held_none_are_left_to_offer() {
        let mut held = HeldPacts::default();
        for pact in PACTS {
            held.add(pact, Seat(0));
        }
        let run = RunFacts {
            holds_ammo_mod: true,
            ..unlocked(true)
        };
        assert!(draw_pact_offer(PACTS, &held, &[], &run, |_| 0.0).is_empty());
    }

    #[test]
    fn a_held_pact_takes_the_boons_it_overrides_off_later_offers_but_keeps_their_stacks() {
        let mut boons = HeldBoons::default();
        let trench_raider = BOONS.iter().find(|b| b.id == "trench_raider").unwrap();
        boons.add(trench_raider);
        let mut pacts = HeldPacts::default();
        let before = RunFacts {
            held_pacts: pacts.ids(),
            ..unlocked(true)
        };
        assert!(eligible(trench_raider, &before));
        pacts.add(pact(COLD_STEEL), Seat(0));
        let after = RunFacts {
            held_pacts: pacts.ids(),
            ..unlocked(true)
        };
        assert!(!eligible(trench_raider, &after));
        assert_eq!(boons.stacks("trench_raider"), 1, "the Stack held stays");

        // Every Boon a Pact overrides is named by a real Pact.
        pacts.0.clear();
        for pact in PACTS {
            pacts.add(pact, Seat(0));
        }
        let all = RunFacts {
            held_pacts: pacts.ids(),
            ..unlocked(true)
        };
        for id in [
            "full_belt",
            "stripper_clips",
            "bottomless_cup",
            "iron_stomach",
            "short_circuit",
            "trench_raider",
            "live_wire",
        ] {
            let boon = BOONS.iter().find(|b| b.id == id).unwrap();
            assert!(!eligible(boon, &all), "{id}");
        }
    }

    #[test]
    fn overkill_drawback_quotes_the_rounds_real_cap() {
        let drawback = PACTS.iter().find(|pact| pact.pact == OVERKILL).unwrap().drawback;
        assert_eq!(drawback(1), "Alive Cap 24 -> 32");
        assert_eq!(drawback(15), "Alive Cap 34 -> 42");
        assert_eq!(drawback(40), "Alive Cap 48 -> 56");
    }

    #[test]
    fn all_twelve_pacts_have_unique_ids_and_names() {
        assert_eq!(PACTS.len(), 12);
        for (i, a) in PACTS.iter().enumerate() {
            assert_ne!(a.id(), a.name(), "an ID isn't a display name");
            assert!(!(a.upside)().is_empty() && !(a.drawback)(1).is_empty());
            for b in &PACTS[i + 1..] {
                assert_ne!(a.id(), b.id());
                assert_ne!(a.name(), b.name());
            }
            // A Pact's entries are its own, apart from every Boon's.
            assert!(!BOONS.iter().any(|boon| boon.name == a.name()));
        }
        assert!(is_pact_source(Source("Cold Steel")));
        assert!(!is_pact_source(Source("Hollow Points")));
    }

    #[test]
    fn hollow_points_x3_with_glass_jaw_is_1_3_times_2() {
        let hollow = BOONS.iter().find(|b| b.id == "hollow_points").unwrap();
        let mut boons = HeldBoons::default();
        for _ in 0..3 {
            boons.add(hollow);
        }
        let mut entries = boons.modifiers();
        entries.extend(holding(GLASS_JAW));
        assert!(close(resolve(Stat::Damage, 1.0, &entries), 1.3 * 2.0));
    }

    #[test]
    fn a_pacts_upside_reaches_only_its_holder_and_its_world_drawback_everyone() {
        let mut held = HeldPacts::default();
        held.add(pact(DEAD_RUN), Seat(1));
        held.add(pact(GOLD_RUSH), Seat(1));
        held.add(pact(GLASS_JAW), Seat(1));
        let holder = held.modifiers(Seat(1));
        let other = held.modifiers(Seat(0));
        // Move speed is Dead Run's upside; every Zombie being a Sprinter is the world's.
        assert!(close(resolve(Stat::MoveSpeed, 1.0, &holder), PACT_DEAD_RUN_MOVE_SPEED_FACTOR));
        assert!(close(resolve(Stat::MoveSpeed, 1.0, &other), 1.0));
        assert!(close(resolve(Stat::SprintDrain, 1.0, &other), 1.0));
        for entries in [&holder, &other] {
            assert!(close(resolve(Stat::SprinterChance, 0.0, entries), 1.0));
            // Gold Rush's price Drawback is the world's; its Points upside is the holder's.
            assert!(close(resolve(Stat::Price, 1.0, entries), PACT_GOLD_RUSH_PRICE_FACTOR));
        }
        assert!(close(resolve(Stat::PointsRate, 0.0, &other), 0.0));
        assert!(resolve(Stat::PointsRate, 0.0, &holder) > 0.0);
        // Glass Jaw's max Health loss is personal.
        assert!(close(resolve(Stat::MaxHealth, 100.0, &other), 100.0));
        assert!(resolve(Stat::MaxHealth, 100.0, &holder) < 100.0);
    }

    #[test]
    fn glass_jaw_doubles_weapon_damage_and_one_swipe_downs_without_juggernog() {
        let entries = holding(GLASS_JAW);
        assert!(close(resolve(Stat::Damage, 20.0, &entries), 40.0));
        let max = resolve(Stat::MaxHealth, PLAYER_MAX_HEALTH, &entries);
        assert!(close(max, 60.0));
        assert!(max <= ZOMBIE_SWIPE_DAMAGE, "one swipe takes it all");
        let mut with_jug = entries.clone();
        with_jug.extend(Perk::Juggernog.modifiers().iter().copied());
        assert!(resolve(Stat::MaxHealth, PLAYER_MAX_HEALTH, &with_jug) > ZOMBIE_SWIPE_DAMAGE);
    }

    #[test]
    fn gold_rush_doubles_points_on_top_of_double_points_and_dears_every_price() {
        use crate::economy::points::award_amount;
        let entries = holding(GOLD_RUSH);
        let extra = resolve(Stat::PointsRate, 0.0, &entries);
        assert_eq!(award_amount(100, false, extra), 200);
        assert_eq!(award_amount(100, true, extra), 300);
        assert_eq!(charged(Some(1000), &entries), Some(1250));
        assert_eq!(charged(Some(BOX_PRICE), &entries), Some(1188));
    }

    #[test]
    fn scorched_earth_blasts_half_max_health_within_two_metres_and_speeds_zombies_up() {
        let blast = scorched_earth_blast(Vec3::new(3.0, 0.0, 4.0), 1000.0, None);
        assert_eq!(blast.splash.damage(0.0), Some(500.0));
        assert_eq!(blast.splash.damage(2.0), Some(500.0));
        assert_eq!(blast.splash.damage(2.1), None);
        assert!(!blast.hurts_player, "it never hurts the Player");
        assert!(blast.crawlers, "it can make Crawlers");
        assert!(blast.ammo_mods.is_empty());
        assert_eq!(blast.at, Vec3::new(3.0, 1.0, 4.0));
        let entries = holding(SCORCHED_EARTH);
        assert!(close(resolve(Stat::ZombieSpeed, 5.0, &entries), 6.0));
    }

    #[test]
    fn open_bar_makes_perks_free_and_kept_and_the_horde_bigger() {
        let entries = holding(OPEN_BAR);
        for perk in Perk::ALL {
            assert_eq!(resolve_points(Stat::PerkPrice, perk.price(crate::state::RunType::Solo), &entries), 0);
        }
        assert_eq!(crate::economy::perk::kept_on_down(&entries), Perk::ALL.len());
        assert_eq!(resolve_count(Stat::HordeSize, 24, &entries), 30);
    }

    #[test]
    fn undying_gives_two_self_revives_and_regen_only_in_the_intermission() {
        use crate::players::last_stand::spares_left;
        use crate::players::player::regen_rate;
        let entries = holding(UNDYING);
        assert_eq!(spares_left(&entries, 0), 2);
        assert_eq!(spares_left(&entries, 2), 0);
        assert_eq!(regen_rate(100.0, true, &entries), 0.0);
        assert!(regen_rate(100.0, false, &entries) > 0.0);
    }

    #[test]
    fn arsenal_upgrades_every_box_weapon_at_twice_the_price_and_teddy_chance() {
        use crate::weapons::arsenal::WeaponId;
        use crate::weapons::mystery_box::{Prize, box_price, comes_out_upgraded, teddy_chance_for};
        let entries = holding(ARSENAL);
        assert!(comes_out_upgraded(
            Prize::Weapon(WeaponId::RayGun),
            &entries,
            0.9999
        ));
        assert_eq!(box_price(BOX_PRICE, &entries), 2 * BOX_PRICE);
        assert!((teddy_chance_for(0.15, &entries) - 0.3).abs() < 1e-6);
    }

    #[test]
    fn overclocked_drops_ammo_mod_cooldowns_and_doubles_pack_a_punch() {
        let entries = holding(OVERCLOCKED);
        assert_eq!(resolve(Stat::AmmoModCooldown, 15.0, &entries), 0.0);
        assert_eq!(
            resolve_points(Stat::PackAPunchPrice, PAP_PRICE, &entries),
            2 * PAP_PRICE
        );
    }

    #[test]
    fn cold_steel_knife_always_kills_and_reserve_halves() {
        use crate::weapons::weapon::knife_hit;
        let entries = holding(COLD_STEEL);
        assert_eq!(knife_hit(KNIFE_DAMAGE, &entries, 0.9999), f32::INFINITY);
        assert_eq!(resolve_count(Stat::ReserveSize, 80, &entries), 40);
    }

    #[test]
    fn endless_belt_feeds_from_reserve_and_takes_max_ammo_out_of_the_bag() {
        let entries = holding(ENDLESS_BELT);
        assert!(resolve(Stat::ReserveFeed, 0.0, &entries) >= 1.0);
        assert!(resolve(Stat::MaxAmmoInBag, 1.0, &entries) < 1.0);
    }

    #[test]
    fn lightning_rod_makes_traps_free_with_no_cooldown_and_halves_drops() {
        let entries = holding(LIGHTNING_ROD);
        assert_eq!(resolve_points(Stat::TrapPrice, TRAP_PRICE, &entries), 0);
        assert_eq!(
            resolve(Stat::TrapCooldown, TRAP_COOLDOWN_SECS, &entries),
            0.0
        );
        assert!(close(resolve(Stat::PowerUpDropChance, 1.0, &entries), 0.5));
    }

    #[test]
    fn overkill_carries_to_the_nearest_zombie_within_five_metres_and_raises_the_alive_cap() {
        let at = Vec3::ZERO;
        let zombies = [
            ("far", Vec3::new(4.0, 1.0, 0.0)),
            ("near", Vec3::new(0.0, 1.0, 3.0)),
            ("out", Vec3::new(5.5, 1.0, 0.0)),
        ];
        assert_eq!(overkill_target(at, 50.0, zombies), Some("near"));
        assert_eq!(overkill_target(at, 0.0, zombies), None, "nothing to carry");
        assert_eq!(overkill_target(at, 50.0, [zombies[2]]), None, "too far");
        let entries = holding(OVERKILL);
        assert_eq!(resolve_count(Stat::AliveCap, ALIVE_CAP, &entries), 32);
    }

    #[test]
    fn dead_run_speeds_the_player_up_with_unlimited_sprint_and_makes_every_zombie_sprint() {
        use crate::run::round::spawn_tier;
        use crate::enemies::zombie::SpeedTier;
        let entries = holding(DEAD_RUN);
        assert!(close(resolve(Stat::MoveSpeed, 10.0, &entries), 13.0));
        assert_eq!(resolve(Stat::SprintDrain, 1.0, &entries), 0.0);
        assert_eq!(
            spawn_tier(SpeedTier::Walker, &entries, 0.9999),
            SpeedTier::Sprinter
        );
    }
}
