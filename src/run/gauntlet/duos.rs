//! Duo Boons: one for each pair of Families, each eligible once its two named recipe Boons are
//! held. A Duo is Legendary, held once with no Stacks, has no Unlock price (it's in the pool once
//! both recipe Boons are Unlocked), stays if a recipe Boon is lost, and counts toward neither
//! Family (spec #182, Appendix A5).

use bevy::prelude::*;

use super::boon::{BoonDef, Family, HeldBoons, Rarity, RunFacts, find};
use crate::modifier::{Effect, Stat};
use crate::tuning::*;

/// A Duo's recipe: the two Boons it needs, by stable ID, and their Families in the same order.
#[derive(Debug, PartialEq)]
pub struct DuoDef {
    pub recipe: [&'static str; 2],
    pub families: [Family; 2],
}

pub const GUN_AND_BLADE: &str = "gun_and_blade";
pub const BALL_LIGHTNING: &str = "ball_lightning";
pub const INCENDIARY_VOLLEY: &str = "incendiary_volley";
pub const GOLD_STANDARD: &str = "gold_standard";
pub const CONSCRIPTS: &str = "conscripts";
pub const STORM_SHELL: &str = "storm_shell";
pub const THERMAL_SHOCK: &str = "thermal_shock";
pub const CUT_OF_THE_TAKE: &str = "cut_of_the_take";
pub const REPEL_BOARDERS: &str = "repel_boarders";
pub const LIVE_FUSE: &str = "live_fuse";
pub const HOUSE_EDGE: &str = "house_edge";
pub const GALVANISED_DEAD: &str = "galvanised_dead";
pub const SUPPLY_DROP: &str = "supply_drop";
pub const MARTYRS: &str = "martyrs";
pub const LIFE_INSURANCE: &str = "life_insurance";

/// A Duo Boon: Legendary, with no Requirement beyond its recipe. Its own Family field is its first
/// recipe Boon's, but a Duo counts toward neither.
const fn duo(
    id: &'static str,
    name: &'static str,
    def: &'static DuoDef,
    per_stack: &'static [(Stat, Effect)],
    effect: fn(u32) -> String,
) -> BoonDef {
    BoonDef {
        id,
        name,
        family: def.families[0],
        rarity: Rarity::Legendary,
        starts_unlocked: false,
        requirements: &[],
        per_stack,
        effect,
        proc: None,
        duo: Some(def),
    }
}

const fn recipe(a: &'static str, family_a: Family, b: &'static str, family_b: Family) -> DuoDef {
    DuoDef { recipe: [a, b], families: [family_a, family_b] }
}

/// Every Duo Boon.
pub const DUOS: &[BoonDef] = &[
    duo(GUN_AND_BLADE, "Gun and Blade", &recipe("marksmans_eye", Family::Lead, "trench_raider", Family::Blade), &[], |_| {
        format!("Headshot kills ready the Knife; Knife kills put {DUO_GUN_AND_BLADE_BULLETS} bullets back")
    }),
    duo(BALL_LIGHTNING, "Ball Lightning", &recipe("parting_shot", Family::Lead, "arc_light", Family::Spark), &[], |_| {
        format!("each Zombie a Parting Shot Blast kills chains Shock to {DUO_BALL_LIGHTNING_CHAIN_ZOMBIES} more")
    }),
    duo(
        INCENDIARY_VOLLEY,
        "Incendiary Volley",
        &recipe("firing_squad", Family::Lead, "hot_brass", Family::Ember),
        &[],
        |_| "your phantom guns' shots set Zombies alight (Paid)".into(),
    ),
    duo(GOLD_STANDARD, "Gold Standard", &recipe("hollow_points", Family::Lead, "blood_money", Family::Gold), &[], |_| {
        format!("Headshot kills pay Blood Money x{DUO_GOLD_STANDARD_FACTOR}")
    }),
    duo(CONSCRIPTS, "Conscripts", &recipe("full_belt", Family::Lead, "turncoat", Family::Grit), &[], |_| {
        format!("each Turned ally fires a phantom gun of {DUO_CONSCRIPTS_SHOTS} shots every {DUO_CONSCRIPTS_INTERVAL_SECS:.0} s")
    }),
    duo(STORM_SHELL, "Storm Shell", &recipe("static_discharge", Family::Blade, "live_wire", Family::Spark), &[], |_| {
        format!(
            "Static Discharge also fires with {DUO_STORM_SHELL_CROWD} Zombies within {DUO_STORM_SHELL_RADIUS_M:.0} m; its cooldown x{DUO_STORM_SHELL_COOLDOWN_FACTOR:.1}"
        )
    }),
    duo(THERMAL_SHOCK, "Thermal Shock", &recipe("frostbite", Family::Blade, "wildfire", Family::Ember), &[], |_| {
        format!("a Frosted Zombie set alight shatters in a {DUO_THERMAL_SHOCK_RADIUS_M:.1} m Blast")
    }),
    duo(
        CUT_OF_THE_TAKE,
        "Cut of the Take",
        &recipe("butchers_bill", Family::Blade, "lucky_strike", Family::Gold),
        &[],
        |_| format!("each Butcher's Bill drop is {DUO_CUT_OF_THE_TAKE_DROPS} Power-ups"),
    ),
    duo(REPEL_BOARDERS, "Repel Boarders", &recipe("elbow_room", Family::Blade, "barbed_wire", Family::Grit), &[], |_| {
        format!(
            "Elbow Room throws Zombies back outside, -{:.0}% health",
            DUO_REPEL_BOARDERS_HEALTH_FRACTION * 100.0
        )
    }),
    duo(LIVE_FUSE, "Live Fuse", &recipe("short_circuit", Family::Spark, "firebrand", Family::Ember), &[], |_| {
        format!("each Firebrand pool Shocks a Zombie in it every {DUO_LIVE_FUSE_TICK_SECS:.0} s")
    }),
    duo(
        HOUSE_EDGE,
        "House Edge",
        &recipe("black_market", Family::Spark, "loaded_dice", Family::Gold),
        &[
            (Stat::BoxUpgradeChance, Effect::Flat(1.0)),
            (Stat::BoxAmmoModRolls, Effect::Flat(DUO_HOUSE_EDGE_AMMO_MODS)),
        ],
        |_| format!("Box Weapons come out Upgraded, with {DUO_HOUSE_EDGE_AMMO_MODS:.0} Ammo Mod"),
    ),
    duo(
        GALVANISED_DEAD,
        "Galvanised Dead",
        &recipe("live_wire", Family::Spark, "raise_the_dead", Family::Grit),
        &[],
        |_| format!("a Zombie that hits your Turned ally is Shocked, chaining to {DUO_GALVANISED_DEAD_CHAIN_ZOMBIES} more"),
    ),
    duo(SUPPLY_DROP, "Supply Drop", &recipe("spoils_of_war", Family::Gold, "bandolier", Family::Ember), &[], |_| {
        format!("a Power-up gives {DUO_SUPPLY_DROP_LETHALS} Lethal and drops a free frag on the thickest crowd")
    }),
    duo(MARTYRS, "Martyrs", &recipe("turncoat", Family::Grit, "shrapnel", Family::Ember), &[], |_| {
        format!("a Turned ally that falls Blasts {DUO_MARTYRS_RADIUS_M:.1} m around it")
    }),
    duo(LIFE_INSURANCE, "Life Insurance", &recipe("war_bonds", Family::Gold, "last_rites", Family::Grit), &[], |_| {
        "going Down pays War Bonds' Round-end Points at once".into()
    }),
];

/// Whether a Boon is Unlocked for this Run's Offers.
fn unlocked(id: &str, run: &RunFacts) -> bool {
    run.unlock_all
        || find(id).is_some_and(|boon| boon.starts_unlocked)
        || run.unlocked.iter().any(|held| held == id)
}

/// Whether a Duo can be on an Offer: both recipe Boons held and Unlocked, and the Duo not held.
pub fn duo_eligible(duo: &BoonDef, held: &HeldBoons, run: &RunFacts) -> bool {
    let Some(def) = duo.duo else {
        return false;
    };
    held.stacks(duo.id) == 0 && def.recipe.iter().all(|&id| held.stacks(id) > 0 && unlocked(id, run))
}

/// Every Duo in `table` the Player is eligible for: what an Offer may draw, and what the Boss
/// Offer's guarantee picks from.
pub fn duo_pool(table: &'static [BoonDef], held: &HeldBoons, run: &RunFacts) -> Vec<&'static BoonDef> {
    table.iter().filter(|duo| duo_eligible(duo, held, run)).collect()
}

/// The Duos a pick of `boon` would make eligible, for its card's "Duo ready" tag: those whose
/// other recipe Boon is held, while the Duo isn't.
pub fn duo_ready(boon: &BoonDef, held: &HeldBoons) -> Vec<&'static BoonDef> {
    if held.stacks(boon.id) > 0 {
        return Vec::new();
    }
    DUOS.iter()
        .filter(|duo| held.stacks(duo.id) == 0)
        .filter(|duo| partner(duo, boon.id).is_some_and(|other| held.stacks(other) > 0))
        .collect()
}

/// The other recipe Boon of `duo`, if `id` is one of its two.
pub fn partner(duo: &BoonDef, id: &str) -> Option<&'static str> {
    let [a, b] = duo.duo?.recipe;
    if a == id {
        Some(b)
    } else if b == id {
        Some(a)
    } else {
        None
    }
}

/// While `boon` is held and a Duo it's in isn't yet eligible, the partner it waits for and the
/// Duo, as (partner's name, Duo's name).
pub fn partners_wanted(boon: &BoonDef, held: &HeldBoons) -> Vec<(&'static str, &'static str)> {
    if held.stacks(boon.id) == 0 {
        return Vec::new();
    }
    DUOS.iter()
        .filter(|duo| held.stacks(duo.id) == 0)
        .filter_map(|duo| {
            let other = partner(duo, boon.id)?;
            (held.stacks(other) == 0).then(|| (find(other).map_or(other, |boon| boon.name), duo.name))
        })
        .collect()
}

/// Gold Standard: Blood Money's Points for a kill, paid again on a Headshot by a holder.
pub fn gold_standard_points(blood_money: u64, headshot: bool, holds: bool) -> u64 {
    if headshot && holds { blood_money.saturating_mul(DUO_GOLD_STANDARD_FACTOR) } else { blood_money }
}

/// Storm Shell: whether `near` Zombies within `DUO_STORM_SHELL_RADIUS_M` are a crowd enough to
/// fire Static Discharge.
pub fn storm_shell_crowded(near: usize) -> bool {
    near >= DUO_STORM_SHELL_CROWD
}

/// Storm Shell: Static Discharge's cooldown for a holder of it.
pub fn storm_shell_cooldown(cooldown: f32, holds: bool) -> f32 {
    if holds { cooldown * DUO_STORM_SHELL_COOLDOWN_FACTOR } else { cooldown }
}

/// Galvanised Dead: whether an ally last answered for at `last` (seconds) may answer again `now`.
pub fn galvanised_ready(last: Option<f32>, now: f32) -> bool {
    last.is_none_or(|last| now - last >= DUO_GALVANISED_DEAD_COOLDOWN_SECS)
}

/// Repel Boarders: the health a Zombie of `max_health` loses as it's flung outside.
pub fn repel_boarders_damage(max_health: f32) -> f32 {
    max_health * DUO_REPEL_BOARDERS_HEALTH_FRACTION
}

/// Supply Drop: where the free frag lands, among `zombies` within `DUO_SUPPLY_DROP_RANGE_M` of
/// `from`: the one with the most others within the frag's splash radius, nearest first on a tie.
pub fn densest_point(from: Vec3, zombies: &[Vec3]) -> Option<Vec3> {
    let radius = FRAG_SPLASH.radius;
    zombies
        .iter()
        .filter(|at| at.distance(from) <= DUO_SUPPLY_DROP_RANGE_M)
        .map(|&at| {
            let crowd = zombies.iter().filter(|other| other.distance(at) <= radius).count();
            (at, crowd)
        })
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.distance(from).total_cmp(&a.0.distance(from))))
        .map(|(at, _)| at)
}

/// Life Insurance: War Bonds' Round-end Points, paid at once on a Down.
pub fn life_insurance_points(round: u32, war_bonds_stacks: u32) -> u64 {
    super::boon::war_bonds_points(round, war_bonds_stacks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::gauntlet::boon::{BOONS, draw_offer};

    fn boon(id: &str) -> &'static BoonDef {
        find(id).unwrap_or_else(|| panic!("no Boon {id}"))
    }

    fn holding(ids: &[&str]) -> HeldBoons {
        let mut held = HeldBoons::default();
        for id in ids {
            held.add(boon(id));
        }
        held
    }

    fn everything() -> RunFacts {
        RunFacts { unlock_all: true, ..default() }
    }

    #[test]
    fn every_duo_from_appendix_a5_is_in_the_table_with_its_recipe() {
        let expected = [
            (GUN_AND_BLADE, ["marksmans_eye", "trench_raider"]),
            (BALL_LIGHTNING, ["parting_shot", "arc_light"]),
            (INCENDIARY_VOLLEY, ["firing_squad", "hot_brass"]),
            (GOLD_STANDARD, ["hollow_points", "blood_money"]),
            (CONSCRIPTS, ["full_belt", "turncoat"]),
            (STORM_SHELL, ["static_discharge", "live_wire"]),
            (THERMAL_SHOCK, ["frostbite", "wildfire"]),
            (CUT_OF_THE_TAKE, ["butchers_bill", "lucky_strike"]),
            (REPEL_BOARDERS, ["elbow_room", "barbed_wire"]),
            (LIVE_FUSE, ["short_circuit", "firebrand"]),
            (HOUSE_EDGE, ["black_market", "loaded_dice"]),
            (GALVANISED_DEAD, ["live_wire", "raise_the_dead"]),
            (SUPPLY_DROP, ["spoils_of_war", "bandolier"]),
            (MARTYRS, ["turncoat", "shrapnel"]),
            (LIFE_INSURANCE, ["war_bonds", "last_rites"]),
        ];
        assert_eq!(DUOS.len(), expected.len());
        for (id, recipe) in expected {
            let duo = boon(id);
            let def = duo.duo.expect("a Duo");
            assert_eq!(def.recipe, recipe, "{id}");
            assert_eq!(duo.rarity, Rarity::Legendary, "{id}");
            for (i, part) in recipe.iter().enumerate() {
                let part = boon(part);
                assert!(part.duo.is_none(), "{id}'s recipe is plain Boons");
                assert_eq!(def.families[i], part.family, "{id}: {} is {:?}", part.id, part.family);
            }
        }
    }

    #[test]
    fn there_is_one_duo_for_each_pair_of_families() {
        let mut pairs: Vec<_> = DUOS
            .iter()
            .map(|duo| {
                let [a, b] = duo.duo.unwrap().families;
                assert_ne!(a, b, "{}", duo.id);
                ((a as usize).min(b as usize), (a as usize).max(b as usize))
            })
            .collect();
        pairs.sort();
        pairs.dedup();
        assert_eq!(pairs.len(), 15, "every pair of the six Families, once");
    }

    #[test]
    fn no_boon_is_in_more_than_two_recipes() {
        for plain in BOONS {
            let count = DUOS.iter().filter(|duo| partner(duo, plain.id).is_some()).count();
            assert!(count <= 2, "{} is in {count} recipes", plain.id);
        }
    }

    #[test]
    fn duo_ids_and_names_never_clash_with_a_boons() {
        for duo in DUOS {
            assert!(BOONS.iter().all(|plain| plain.id != duo.id && plain.name != duo.name), "{}", duo.id);
        }
    }

    #[test]
    fn a_duo_is_eligible_once_both_its_recipe_boons_are_held() {
        let duo = boon(GOLD_STANDARD);
        let run = everything();
        assert!(!duo_eligible(duo, &holding(&[]), &run));
        assert!(!duo_eligible(duo, &holding(&["hollow_points"]), &run));
        assert!(duo_eligible(duo, &holding(&["hollow_points", "blood_money"]), &run));
        assert!(!duo_eligible(duo, &holding(&["hollow_points", "blood_money", GOLD_STANDARD]), &run), "held once");
    }

    #[test]
    fn a_duo_needs_both_recipe_boons_unlocked_and_has_no_price_of_its_own() {
        let duo = boon(BALL_LIGHTNING);
        let held = holding(&["parting_shot", "arc_light"]);
        let one = RunFacts { unlocked: vec!["parting_shot".into()], ..default() };
        assert!(!duo_eligible(duo, &held, &one));
        let both = RunFacts { unlocked: vec!["parting_shot".into(), "arc_light".into()], ..default() };
        assert!(duo_eligible(duo, &held, &both), "the Duo's own id never needs buying");
        // A recipe of starting Boons is in the pool from the first launch.
        let starters = holding(&["hollow_points", "blood_money"]);
        assert!(duo_eligible(boon(GOLD_STANDARD), &starters, &RunFacts::default()));
    }

    #[test]
    fn the_duo_pool_holds_every_eligible_duo_and_nothing_else() {
        let held = holding(&["turncoat", "full_belt", "shrapnel", "hollow_points"]);
        let ids: Vec<_> = duo_pool(DUOS, &held, &everything()).iter().map(|duo| duo.id).collect();
        assert_eq!(ids, [CONSCRIPTS, MARTYRS]);
    }

    #[test]
    fn an_eligible_duo_turns_up_on_offers_as_a_legendary() {
        let held = holding(&["hollow_points", "blood_money"]);
        let run = everything();
        let mut seen = 0;
        for i in 0..200 {
            let mut state = i as f64 / 200.0;
            let offer = draw_offer(BOONS, &held, &run, |total| {
                state = (state * 7.31 + 0.137).fract();
                state * total
            });
            seen += offer.iter().filter(|card| card.id == GOLD_STANDARD).count();
        }
        assert!(seen > 0, "Gold Standard never came up");
        let never = holding(&["hollow_points"]);
        for _ in 0..50 {
            let offer = draw_offer(BOONS, &never, &run, |total| rand::random_range(0.0..total));
            assert!(offer.iter().all(|card| card.duo.is_none()), "a Duo came up without its recipe");
        }
    }

    #[test]
    fn duos_count_toward_neither_family() {
        let held = holding(&["hollow_points", "blood_money", GOLD_STANDARD]);
        assert_eq!(held.in_family(Family::Lead), 1);
        assert_eq!(held.in_family(Family::Gold), 1);
    }

    #[test]
    fn a_pick_that_completes_a_recipe_is_duo_ready() {
        let held = holding(&["turncoat"]);
        let ready: Vec<_> = duo_ready(boon("shrapnel"), &held).iter().map(|duo| duo.id).collect();
        assert_eq!(ready, [MARTYRS]);
        assert!(duo_ready(boon("turncoat"), &held).is_empty(), "already held");
        assert!(duo_ready(boon("hollow_points"), &held).is_empty(), "in no recipe with Turncoat");
        let done = holding(&["turncoat", MARTYRS]);
        assert!(duo_ready(boon("shrapnel"), &done).is_empty(), "the Duo is held already");
    }

    #[test]
    fn a_held_recipe_boon_names_the_partner_it_waits_for() {
        let held = holding(&["turncoat"]);
        assert_eq!(partners_wanted(boon("turncoat"), &held), [("Full Belt", "Conscripts"), ("Shrapnel", "Martyrs")]);
        let paired = holding(&["turncoat", "shrapnel"]);
        assert_eq!(partners_wanted(boon("turncoat"), &paired), [("Full Belt", "Conscripts")]);
        assert!(partners_wanted(boon("shrapnel"), &held).is_empty(), "not held");
    }

    #[test]
    fn gold_standard_pays_blood_money_twice_on_a_headshot() {
        assert_eq!(gold_standard_points(20, true, true), 40);
        assert_eq!(gold_standard_points(20, false, true), 20);
        assert_eq!(gold_standard_points(20, true, false), 20);
        assert_eq!(gold_standard_points(0, true, true), 0, "an Unpaid kill stays unpaid");
    }

    #[test]
    fn storm_shell_fires_on_a_crowd_of_five_and_halves_the_cooldown() {
        assert!(!storm_shell_crowded(4));
        assert!(storm_shell_crowded(5));
        assert_eq!(storm_shell_cooldown(30.0, true), 15.0);
        assert_eq!(storm_shell_cooldown(30.0, false), 30.0);
    }

    #[test]
    fn galvanised_dead_answers_for_one_ally_at_most_every_two_seconds() {
        assert!(galvanised_ready(None, 0.0));
        assert!(!galvanised_ready(Some(1.0), 2.9));
        assert!(galvanised_ready(Some(1.0), 3.0));
    }

    #[test]
    fn repel_boarders_takes_a_tenth_of_max_health() {
        assert!((repel_boarders_damage(500.0) - 50.0).abs() < 1e-4);
    }

    #[test]
    fn supply_drop_lands_on_the_thickest_crowd_in_range() {
        let from = Vec3::ZERO;
        let crowd = Vec3::new(6.0, 0.0, 0.0);
        let zombies = [Vec3::new(-6.0, 0.0, 0.0), crowd, crowd + Vec3::X * 0.5, crowd - Vec3::Z * 0.5, Vec3::new(30.0, 0.0, 0.0)];
        let landed = densest_point(from, &zombies).unwrap();
        assert!(landed.distance(crowd) < 1.0, "{landed}");
        assert_eq!(densest_point(from, &[Vec3::new(30.0, 0.0, 0.0)]), None, "nothing in range");
    }

    #[test]
    fn life_insurance_pays_war_bonds_round_end_points() {
        assert_eq!(life_insurance_points(7, 2), BOON_WAR_BONDS_POINTS_PER_ROUND * 7 * 2);
        assert_eq!(life_insurance_points(7, 0), 0);
    }
}
