//! Set Bonuses at work in a Run: tiers reached, their rules and their capstones, between two
//! Players in one world with no netcode.

use super::coop_tests::{gauntlet_players_with, give_stacks, killed};
use super::boon::Family;
use super::sets::*;
use super::*;
use crate::economy::points::{PointsAwarded, kill_points};
use crate::economy::power_up::ActivePowerUps;
use crate::enemies::zombie::headless::Sim;
use crate::enemies::zombie::{Attack, DamageTakenFactor, Pay};
use crate::players::player::two_players::place;
use crate::players::player::{DownGuard, HealPlayer};
use crate::run::round::RoundStarted;
use crate::weapons::ammo_mod::{CastEffect, CastTarget, Element};
use crate::weapons::arsenal::Burn;
use crate::weapons::burn::Burning;
use crate::weapons::weapon::{KnifeSwung, ResetKnifeCooldown};

const LEAD: [&str; 6] = ["hollow_points", "marksmans_eye", "full_belt", "stripper_clips", "parting_shot", "third_hand"];
const BLADE: [&str; 6] = ["trench_raider", "butchers_bill", "light_feet", "long_arm", "frostbite", "elbow_room"];
const EMBER: [&str; 4] = ["bandolier", "kindling", "black_powder", "shrapnel"];
/// Gold Boons that pay no kill bonus of their own.
const GOLD: [&str; 6] = ["teddys_tithe", "bottomless_cup", "long_fuse", "scrounger", "loaded_dice", "war_bonds"];
const GRIT: [&str; 6] = ["second_wind", "iron_stomach", "last_rites", "union_rates", "barbed_wire", "turncoat"];

/// Two Gauntlet Players with every Set Bonus system running.
fn players() -> (Sim, Entity, Entity) {
    gauntlet_players_with(|app| {
        app.add_message::<SetTierReached>()
            .add_message::<HealPlayer>()
            .add_message::<ResetKnifeCooldown>()
            .add_message::<KnifeSwung>()
            .add_message::<CastEffect>()
            .add_message::<PointsAwarded>()
            .init_resource::<SetFlashes>()
            .init_resource::<ActivePowerUps>()
            .add_systems(
                Update,
                (
                    reach_set_bonuses,
                    (blade_4, ember_4, gold_4, grit_4, ricochet, cleave, jackpot, bodyguard, scorched),
                )
                    .chain(),
            );
    })
}

fn hold(sim: &mut Sim, player: Entity, ids: &[&str]) {
    for id in ids {
        give_stacks(sim, player, id, 1);
    }
    sim.run(2);
}

fn drained<M: Message>(sim: &mut Sim) -> Vec<M> {
    sim.world_mut().resource_mut::<Messages<M>>().drain().collect()
}

#[test]
fn a_tier_reached_is_told_and_puts_its_entries_on_its_player_alone() {
    let (mut sim, local, teammate) = players();
    drained::<SetTierReached>(&mut sim);
    give_stacks(&mut sim, local, LEAD[0], 1);
    give_stacks(&mut sim, local, LEAD[1], 1);
    sim.run(2);
    let reached = drained::<SetTierReached>(&mut sim);
    assert_eq!(reached, [SetTierReached { player: local, family: Family::Lead, tier: 2 }]);
    let has_lead_2 = |sim: &mut Sim, player| {
        sim.world_mut().get::<Modifiers>(player).unwrap().0.iter().any(|entry| entry.source.0 == "Lead 2")
    };
    assert!(has_lead_2(&mut sim, local));
    assert!(!has_lead_2(&mut sim, teammate));
}

#[test]
fn blade_4_readies_the_knife_and_heals_on_a_knife_kill_only() {
    let (mut sim, local, _) = players();
    hold(&mut sim, local, &BLADE[..4]);
    drained::<ResetKnifeCooldown>(&mut sim);
    sim.send(killed(local, Pay::Paid, Attack::Bullet));
    sim.run(1);
    assert!(drained::<ResetKnifeCooldown>(&mut sim).is_empty());
    sim.send(killed(local, Pay::Paid, Attack::Knife));
    sim.run(1);
    assert_eq!(drained::<ResetKnifeCooldown>(&mut sim).len(), 1);
    let heals = drained::<HealPlayer>(&mut sim);
    assert_eq!(heals.len(), 1);
    assert_eq!((heals[0].player, heals[0].amount), (local, SET_BLADE_4_HEAL));
}

#[test]
fn grit_4_gives_a_down_guard_and_refills_it_at_each_round_start() {
    let (mut sim, local, teammate) = players();
    hold(&mut sim, local, &GRIT[..4]);
    assert_eq!(sim.world_mut().get::<DownGuard>(local).unwrap().charges, 1);
    assert!(sim.world_mut().get::<DownGuard>(teammate).is_none());
    sim.world_mut().get_mut::<DownGuard>(local).unwrap().charges = 0;
    sim.send(RoundStarted);
    sim.run(2);
    assert_eq!(sim.world_mut().get::<DownGuard>(local).unwrap().charges, 1);
}

#[test]
fn gold_4_pays_a_kill_again_while_a_timed_power_up_is_on() {
    let (mut sim, local, _) = players();
    hold(&mut sim, local, &GOLD[..4]);
    drained::<BonusPoints>(&mut sim);
    let kill = killed(local, Pay::Paid, Attack::Bullet);
    sim.send(kill);
    sim.run(1);
    assert!(drained::<BonusPoints>(&mut sim).is_empty(), "no timed Power-up on");
    sim.world_mut().resource_mut::<ActivePowerUps>().start(PowerUp::DoublePoints, 0.0);
    sim.send(kill);
    sim.run(1);
    let paid: Vec<_> = drained::<BonusPoints>(&mut sim).iter().map(|bonus| (bonus.player, bonus.amount)).collect();
    assert_eq!(paid, [(local, kill_points(&kill))]);
}

#[test]
fn ember_4_makes_a_burning_zombie_take_more_damage_while_it_burns() {
    let (mut sim, local, _) = players();
    hold(&mut sim, local, &EMBER);
    let zombie = sim.spawn(Vec3::new(4.0, 0.0, 12.0), None);
    sim.run(2);
    assert!(sim.world_mut().get::<DamageTakenFactor>(zombie).is_none());
    let burn = Burn { damage: 1.0, tick_secs: 0.5, secs: 30.0 };
    sim.world_mut().entity_mut(zombie).insert(Burning::new(burn));
    sim.run(2);
    let factor = *sim.world_mut().get::<DamageTakenFactor>(zombie).expect("burning");
    assert_eq!(factor, DamageTakenFactor { body: 1.25, head: 1.25 });
    sim.world_mut().entity_mut(zombie).remove::<Burning>();
    sim.run(2);
    assert!(sim.world_mut().get::<DamageTakenFactor>(zombie).is_none(), "put out");
}

#[test]
fn ricochet_carries_a_headshot_kills_damage_to_the_nearest_other_head() {
    let (mut sim, local, _) = players();
    hold(&mut sim, local, &LEAD);
    let dead = sim.spawn(Vec3::new(4.0, 0.0, 12.0), None);
    let near = sim.spawn(Vec3::new(6.0, 0.0, 12.0), None);
    sim.spawn(Vec3::new(4.0, 0.0, 19.0), None);
    sim.run(2);
    drained::<DamageZombie>(&mut sim);
    let kill = |headshot| KillDamage {
        player: Some(local),
        zombie: dead,
        at: Vec3::new(4.0, 0.0, 12.0),
        max_health: 150.0,
        overflow: 50.0,
        attack: Attack::Bullet,
        damage: 200.0,
        headshot,
    };
    sim.send(kill(false));
    sim.run(1);
    assert!(drained::<DamageZombie>(&mut sim).is_empty(), "only a Headshot kill bounces");
    sim.send(kill(true));
    sim.run(1);
    let bounced = drained::<DamageZombie>(&mut sim);
    assert_eq!(bounced.len(), 1);
    assert_eq!((bounced[0].zombie, bounced[0].damage), (near, 200.0));
    assert_eq!((bounced[0].zone, bounced[0].attack), (HitZone::Head, Attack::AmmoMod(Pay::Paid)));
}

#[test]
fn cleave_strikes_every_other_zombie_in_the_swings_arc_and_leaves_frost() {
    let (mut sim, local, _) = players();
    hold(&mut sim, local, &BLADE);
    let feet = Vec3::new(0.0, 0.0, 12.0);
    place(&mut sim, local, feet);
    let struck = sim.spawn(feet + Vec3::new(0.0, 0.0, -1.2), None);
    let beside = sim.spawn(feet + Vec3::new(0.9, 0.0, -0.9), None);
    let behind = sim.spawn(feet + Vec3::new(0.0, 0.0, 1.2), None);
    sim.run(2);
    drained::<DamageZombie>(&mut sim);
    sim.send(KnifeSwung {
        player: local,
        origin: feet + Vec3::Y * PLAYER_EYE_HEIGHT,
        forward: Vec3::NEG_Z,
        range: KNIFE_RANGE,
        damage: 150.0,
        struck: Some(struck),
    });
    sim.run(1);
    let hits = drained::<DamageZombie>(&mut sim);
    let hit: Vec<_> = hits.iter().map(|hit| hit.zombie).collect();
    assert_eq!(hit, [beside], "not the one the Knife struck, nor the one behind ({behind:?})");
    assert_eq!((hits[0].damage, hits[0].attack), (150.0, Attack::AmmoMod(Pay::Paid)));
    let patches = sim.world_mut().query::<&FrostPatch>().iter(sim.world_mut()).count();
    assert_eq!(patches, 1, "a Frost trail is left");
}

#[test]
fn jackpot_drops_a_power_up_every_four_thousand_points_earned_after_gold_6() {
    let (mut sim, local, teammate) = players();
    sim.send(PointsAwarded { player: local, amount: 5000 });
    sim.run(1);
    hold(&mut sim, local, &GOLD);
    drained::<DropPowerUp>(&mut sim);
    sim.send(PointsAwarded { player: local, amount: 3000 });
    sim.send(PointsAwarded { player: teammate, amount: 9000 });
    sim.run(1);
    assert!(drained::<DropPowerUp>(&mut sim).is_empty(), "Points before Gold 6 don't count");
    sim.send(PointsAwarded { player: local, amount: 1000 });
    sim.run(1);
    assert_eq!(drained::<DropPowerUp>(&mut sim).len(), 1);
}

#[test]
fn the_bodyguard_is_the_first_zombie_to_spawn_after_grit_6_turned_for_good() {
    let (mut sim, local, _) = players();
    let before = sim.spawn(Vec3::new(4.0, 0.0, 12.0), None);
    sim.run(2);
    hold(&mut sim, local, &GRIT);
    drained::<CastEffect>(&mut sim);
    let guard = sim.spawn(Vec3::new(-4.0, 0.0, 12.0), None);
    sim.spawn(Vec3::new(-6.0, 0.0, 12.0), None);
    // Messages last two frames.
    sim.run(2);
    let casts = drained::<CastEffect>(&mut sim);
    assert_eq!(casts.len(), 1);
    assert_eq!(casts[0].target, CastTarget::Zombie(guard), "not {before:?}, which spawned before");
    assert_eq!(casts[0].element, Element::Turned);
    assert!(casts[0].unbounded);
    assert_eq!(casts[0].tint, Some(Family::Grit.colour()));
    // Never Turned (no Ammo Mod plugin here), so a new Round owes another.
    sim.send(RoundStarted);
    sim.run(1);
    let next = sim.spawn(Vec3::new(-8.0, 0.0, 12.0), None);
    sim.run(2);
    let casts = drained::<CastEffect>(&mut sim);
    assert_eq!(casts.iter().map(|cast| cast.target).collect::<Vec<_>>(), [CastTarget::Zombie(next)]);
}

#[test]
fn scorched_leaves_a_fire_patch_where_a_player_kill_fell() {
    let (mut sim, local, _) = players();
    hold(&mut sim, local, &["bandolier", "kindling", "black_powder", "shrapnel", "hot_brass", "firebrand"]);
    drained::<CastEffect>(&mut sim);
    sim.send(killed(local, Pay::Paid, Attack::Burn));
    sim.run(1);
    assert!(drained::<CastEffect>(&mut sim).is_empty(), "only Player kills");
    sim.send(killed(local, Pay::Paid, Attack::Knife));
    sim.run(1);
    let casts = drained::<CastEffect>(&mut sim);
    assert_eq!(casts.len(), 1);
    assert_eq!((casts[0].element, casts[0].size, casts[0].lasts), (Element::Fire, SET_EMBER_6_PATCH_RADIUS_M, SET_EMBER_6_PATCH_SECS));
    assert_eq!(casts[0].pay, Pay::Paid);
}
