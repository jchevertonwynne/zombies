//! Duo Boons at work in a Run, between two Players in one world with no netcode: each Duo
//! touches only its holder, through Classic's messages and requests.

use super::coop_tests::{gauntlet_players_with, give_stacks, killed};
use super::duos::*;
use super::procs::ProcFlashes;
use super::sets::SetFlashes;
use super::*;
use crate::enemies::zombie::headless::Sim;
use crate::enemies::zombie::{Attack, DamageZombie, Pay, TurnedAllyHit};
use crate::players::last_stand::{Down, DownOutcome};
use crate::weapons::ammo_mod::{CastEffect, CastTarget, Element, TurnedAlly, TurnedAllyExpired};
use crate::weapons::burn::Ignite;
use crate::weapons::equipment::GiveLethals;
use crate::weapons::weapon::{RefillMagazine, ResetKnifeCooldown};

/// Two Gauntlet Players with every Duo system running.
fn players() -> (Sim, Entity, Entity) {
    gauntlet_players_with(|app| {
        app.add_message::<ResetKnifeCooldown>()
            .add_message::<RefillMagazine>()
            .add_message::<GiveLethals>()
            .add_message::<CastEffect>()
            .add_message::<TurnedAllyExpired>()
            .add_message::<Ignite>()
            .init_resource::<ProcFlashes>()
            .init_resource::<SetFlashes>()
            .init_resource::<BallLightning>()
            .add_systems(
                Update,
                (gun_and_blade, life_insurance, martyrs, thermal_shock, galvanised_dead, conscripts, supply_drop),
            );
    })
}

fn hold(sim: &mut Sim, player: Entity, ids: &[&str]) {
    for id in ids {
        give_stacks(sim, player, id, 1);
    }
    sim.run(1);
}

fn drained<M: Message>(sim: &mut Sim) -> Vec<M> {
    sim.world_mut().resource_mut::<Messages<M>>().drain().collect()
}

fn headshot(by: Entity) -> ZombieKilled {
    ZombieKilled { headshot: true, ..killed(by, Pay::Paid, Attack::Bullet) }
}

/// A Turned ally of `player`'s standing at `at`.
fn ally(sim: &mut Sim, player: Entity, at: Vec3) -> Entity {
    sim.world_mut().spawn((TurnedAlly::of(player), Transform::from_translation(at + Vec3::Y))).id()
}

#[test]
fn gun_and_blade_readies_the_knife_on_a_headshot_and_refills_on_a_knife_kill() {
    let (mut sim, local, teammate) = players();
    hold(&mut sim, local, &[GUN_AND_BLADE]);
    sim.send(headshot(local));
    sim.send(killed(local, Pay::Paid, Attack::Knife));
    sim.send(headshot(teammate));
    sim.run(1);
    let resets: Vec<_> = drained::<ResetKnifeCooldown>(&mut sim).iter().map(|reset| reset.player).collect();
    assert_eq!(resets, [local], "its holder's alone");
    let refills: Vec<_> = drained::<RefillMagazine>(&mut sim).iter().map(|refill| (refill.player, refill.bullets)).collect();
    assert_eq!(refills, [(local, DUO_GUN_AND_BLADE_BULLETS)]);
}

#[test]
fn gold_standard_pays_blood_money_again_on_a_headshot() {
    let (mut sim, local, _) = players();
    hold(&mut sim, local, &["blood_money", GOLD_STANDARD]);
    drained::<BonusPoints>(&mut sim);
    sim.send(killed(local, Pay::Paid, Attack::Bullet));
    sim.run(1);
    let body: u64 = drained::<BonusPoints>(&mut sim).iter().map(|bonus| bonus.amount).sum();
    sim.send(headshot(local));
    sim.run(1);
    let head: u64 = drained::<BonusPoints>(&mut sim).iter().map(|bonus| bonus.amount).sum();
    assert_eq!(body, BOON_BLOOD_MONEY_POINTS_PER_KILL);
    assert_eq!(head, BOON_BLOOD_MONEY_POINTS_PER_KILL * DUO_GOLD_STANDARD_FACTOR);
}

#[test]
fn cut_of_the_take_doubles_every_butchers_bill_drop() {
    let (mut sim, local, _) = players();
    // Enough Stacks that Butcher's Bill all but always drops.
    give_stacks(&mut sim, local, "butchers_bill", 400);
    hold(&mut sim, local, &[CUT_OF_THE_TAKE]);
    drained::<DropPowerUp>(&mut sim);
    sim.send(killed(local, Pay::Paid, Attack::Knife));
    sim.run(1);
    assert_eq!(drained::<DropPowerUp>(&mut sim).len(), DUO_CUT_OF_THE_TAKE_DROPS as usize);
}

#[test]
fn life_insurance_pays_war_bonds_at_once_on_a_down() {
    let (mut sim, local, teammate) = players();
    give_stacks(&mut sim, local, "war_bonds", 2);
    hold(&mut sim, local, &[LIFE_INSURANCE]);
    hold(&mut sim, teammate, &["war_bonds"]);
    drained::<BonusPoints>(&mut sim);
    sim.send(Down { player: local, outcome: DownOutcome::LastStand });
    sim.send(Down { player: teammate, outcome: DownOutcome::LastStand });
    sim.run(1);
    let paid: Vec<_> = drained::<BonusPoints>(&mut sim).iter().map(|bonus| (bonus.player, bonus.amount)).collect();
    assert!(paid.contains(&(local, life_insurance_points(3, 2))), "{paid:?}");
    assert!(paid.iter().all(|&(player, _)| player == local), "War Bonds alone pays nothing on a Down");
}

#[test]
fn martyrs_blasts_around_a_holders_fallen_ally() {
    let (mut sim, local, teammate) = players();
    hold(&mut sim, local, &[MARTYRS]);
    let near = sim.spawn(Vec3::new(4.0, 0.0, 12.0), None);
    sim.run(2);
    drained::<DamageZombie>(&mut sim);
    let at = Vec3::new(4.0, 1.0, 13.0);
    sim.send(TurnedAllyExpired { at, player: Some(teammate) });
    sim.run(1);
    assert!(drained::<DamageZombie>(&mut sim).iter().all(|hit| hit.zombie != near), "not the holder's ally");
    sim.send(TurnedAllyExpired { at, player: Some(local) });
    sim.run(1);
    let hits = drained::<DamageZombie>(&mut sim);
    assert!(hits.iter().any(|hit| hit.zombie == near && hit.attack == Attack::AmmoMod(Pay::Paid) && hit.player == Some(local)));
}

#[test]
fn galvanised_dead_shocks_the_zombie_that_hits_a_holders_ally_at_most_every_two_seconds() {
    let (mut sim, local, _) = players();
    hold(&mut sim, local, &[GALVANISED_DEAD]);
    let ally = ally(&mut sim, local, Vec3::new(4.0, 0.0, 12.0));
    let attacker = sim.spawn(Vec3::new(9.0, 0.0, 12.0), None);
    sim.run(2);
    assert!(sim.world_mut().get::<TurnedAlly>(ally).is_some(), "the ally stands");
    drained::<CastEffect>(&mut sim);
    sim.send(TurnedAllyHit { ally, by: attacker });
    sim.run(1);
    let casts = drained::<CastEffect>(&mut sim);
    assert_eq!(casts.len(), 1);
    assert_eq!((casts[0].element, casts[0].target, casts[0].pay), (Element::Shock, CastTarget::Zombie(attacker), Pay::Paid));
    assert_eq!(casts[0].size, DUO_GALVANISED_DEAD_CHAIN_ZOMBIES as f32);
    sim.send(TurnedAllyHit { ally, by: attacker });
    sim.run(1);
    assert!(drained::<CastEffect>(&mut sim).is_empty(), "too soon after the last");
    sim.run(sim.frames(DUO_GALVANISED_DEAD_COOLDOWN_SECS));
    sim.send(TurnedAllyHit { ally, by: attacker });
    sim.run(1);
    assert_eq!(drained::<CastEffect>(&mut sim).len(), 1);
}

#[test]
fn conscripts_gives_a_holders_ally_a_phantom_gun_every_six_seconds() {
    let (mut sim, local, _) = players();
    hold(&mut sim, local, &[CONSCRIPTS]);
    ally(&mut sim, local, Vec3::new(4.0, 0.0, 12.0));
    sim.run(2);
    drained::<CastEffect>(&mut sim);
    let mut volleys = Vec::new();
    for _ in 0..sim.frames(DUO_CONSCRIPTS_INTERVAL_SECS * 2.0 + 0.2) {
        sim.run(1);
        volleys.extend(drained::<CastEffect>(&mut sim));
    }
    assert_eq!(volleys.len(), 2);
    for volley in volleys {
        assert_eq!((volley.element, volley.size, volley.pay), (Element::PhantomGun, DUO_CONSCRIPTS_SHOTS as f32, Pay::Unpaid));
        assert_eq!(volley.player, local);
    }
}

#[test]
fn supply_drop_gives_a_lethal_on_a_power_up() {
    let (mut sim, local, teammate) = players();
    hold(&mut sim, local, &[SUPPLY_DROP]);
    sim.send(PowerUpTaken(PowerUp::MaxAmmo, Some(local)));
    sim.send(PowerUpTaken(PowerUp::MaxAmmo, Some(teammate)));
    sim.run(1);
    let given: Vec<_> = drained::<GiveLethals>(&mut sim).iter().map(|gift| (gift.player, gift.count)).collect();
    assert_eq!(given, [(local, DUO_SUPPLY_DROP_LETHALS)]);
}

#[test]
fn thermal_shock_shatters_a_frosted_zombie_a_holder_sets_alight() {
    use crate::weapons::ammo_mod::blast_furnace_burn;
    let (mut sim, local, _) = players();
    hold(&mut sim, local, &[THERMAL_SHOCK]);
    let frosted = sim.spawn(Vec3::new(4.0, 0.0, 12.0), None);
    let plain = sim.spawn(Vec3::new(12.0, 0.0, 12.0), None);
    sim.run(2);
    let mut commands_world = sim.world_mut().commands();
    super::procs::frost(&mut commands_world, frosted, None, None, 5.0);
    sim.world_mut().flush();
    drained::<DamageZombie>(&mut sim);
    sim.send(Ignite { player: local, zombie: plain, burn: blast_furnace_burn(100.0) });
    sim.run(1);
    assert!(drained::<DamageZombie>(&mut sim).is_empty(), "an unfrosted Zombie only burns");
    sim.send(Ignite { player: local, zombie: frosted, burn: blast_furnace_burn(100.0) });
    sim.run(1);
    assert!(drained::<DamageZombie>(&mut sim).iter().any(|hit| hit.zombie == frosted && hit.attack == Attack::AmmoMod(Pay::Paid)));
}
