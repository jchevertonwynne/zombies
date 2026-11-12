//! Co-op Downs and Revives between two Players in one world.

use super::*;
use bevy::ecs::system::RunSystemOnce;
use crate::weapons::arsenal::WeaponId;
use crate::players::last_stand::{Down, LastStand, LastStandPlugin};
use crate::economy::perk::{Perk, Perks};
use crate::players::revive::{Revived, Reviving, not_reviving, revive};
use crate::state::RunType;
use crate::run::stats::{RunStats, StatsPlugin};

/// Two Players in a Co-op Run, standing a metre apart, each with a Quick Revive's worth of nothing
/// yet: no Perks. Returns the Sim, the reviver (the local Player) and the teammate.
pub(super) fn co_op() -> (Sim, Entity, Entity) {
    co_op_with(|_| {})
}

/// The same, with `configure` adding whatever else the test needs.
pub(super) fn co_op_with(configure: impl FnOnce(&mut App)) -> (Sim, Entity, Entity) {
    let (mut sim, local, teammate) = two_players_with(|app| {
        configure(app);
        app.insert_resource(RunType::Coop)
            .add_plugins((LastStandPlugin, StatsPlugin))
            .add_message::<Revived>()
            .init_resource::<Seen>()
            .add_systems(Update, (take_hits, revive.after(crate::players::last_stand::go_down), see_revives.after(revive)));
    });
    let world = sim.world_mut();
    for player in [local, teammate] {
        world.entity_mut(player).insert(Perks::default());
    }
    set_gap(&mut sim, local, teammate, 1.0);
    (sim, local, teammate)
}

/// Walks the local Player along the row the Seats stand in until they're about `gap` metres from
/// the teammate. The Seats start three metres apart, out of Revive reach.
pub(super) fn set_gap(sim: &mut Sim, local: Entity, teammate: Entity, gap: f32) {
    let x = |sim: &mut Sim, player| sim.world_mut().get::<Transform>(player).unwrap().translation.x;
    for _ in 0..sim.frames(5.0) {
        let (from, to) = (x(sim, local), x(sim, teammate));
        let distance = (to - from).abs();
        if (distance - gap).abs() < 0.1 {
            break;
        }
        let towards = (to - from).signum() * if distance > gap { 1.0 } else { -1.0 };
        sim.world_mut().get_mut::<MoveInput>(local).unwrap().axes = Vec2::new(towards, 0.0);
        sim.run(1);
    }
    sim.world_mut().get_mut::<MoveInput>(local).unwrap().axes = Vec2::ZERO;
    sim.run(20);
}

/// Every Revive so far, as (revived, reviver).
#[derive(Resource, Default)]
struct Seen(Vec<(Entity, Entity)>);

fn see_revives(mut revived: MessageReader<Revived>, mut seen: ResMut<Seen>) {
    seen.0.extend(revived.read().map(|r| (r.player, r.reviver)));
}

pub(super) fn down(sim: &mut Sim, player: Entity) {
    sim.world_mut().get_mut::<Health>(player).unwrap().current = 0.0;
    sim.run(2);
}

pub(super) fn in_last_stand(sim: &mut Sim, player: Entity) -> bool {
    sim.world_mut().get::<LastStand>(player).is_some()
}

/// `reviver` holds Use on `target` for `secs`.
pub(super) fn hold(sim: &mut Sim, reviver: Entity, target: Entity, secs: f32) {
    for _ in 0..sim.frames(secs) {
        sim.send(UseRequest { player: reviver, buyable: target, hold: true });
        sim.run(1);
    }
}

#[test]
fn a_co_op_down_starts_a_last_stand_and_costs_every_perk_and_five_percent() {
    let (mut sim, local, teammate) = co_op();
    let world = sim.world_mut();
    world.get_mut::<Points>(teammate).unwrap().balance = 1_000;
    world.get_mut::<Perks>(teammate).unwrap().0 = vec![Perk::Juggernog, Perk::DoubleTap];
    world.get_mut::<Loadout>(teammate).unwrap().give(WeaponId::Mp40);
    down(&mut sim, teammate);

    let world = sim.world_mut();
    assert!(world.get::<LastStand>(teammate).is_some(), "no Last Stand");
    assert!(world.get::<LastStand>(local).is_none());
    assert!(world.get::<Perks>(teammate).unwrap().0.is_empty(), "kept a Perk");
    assert_eq!(world.get::<Points>(teammate).unwrap().balance, 950);
    assert_eq!(world.get::<Loadout>(teammate).unwrap().held().stats().name, "Mustang & Sally", "Weapons weren't set aside");
    let downs: Vec<_> = world.resource_mut::<Messages<Down>>().drain().map(|down| down.player).collect();
    assert_eq!(downs, [teammate]);
    assert_eq!(world.get::<RunStats>(teammate).unwrap().downs, 1);
}

#[test]
fn a_co_op_down_with_quick_revive_costs_the_same_and_gives_no_self_revive() {
    let (mut sim, _, teammate) = co_op();
    let world = sim.world_mut();
    world.get_mut::<Points>(teammate).unwrap().balance = 2_000;
    world.get_mut::<Perks>(teammate).unwrap().0 = vec![Perk::QuickRevive];
    down(&mut sim, teammate);
    assert_eq!(sim.world_mut().get::<Points>(teammate).unwrap().balance, 1_900);
    assert!(sim.world_mut().get::<Perks>(teammate).unwrap().0.is_empty());
    // Well past the solo Last Stand's ten seconds, nobody has got them up.
    sim.run(sim.frames(15.0));
    assert!(in_last_stand(&mut sim, teammate), "got up alone");
    assert_eq!(sim.world_mut().get::<Health>(teammate).unwrap().current, 0.0);
}

#[test]
fn a_revive_takes_three_seconds() {
    let (mut sim, local, teammate) = co_op();
    down(&mut sim, teammate);
    hold(&mut sim, local, teammate, 2.9);
    assert!(in_last_stand(&mut sim, teammate), "up after 2.9 s");
    hold(&mut sim, local, teammate, 0.2);
    assert!(!in_last_stand(&mut sim, teammate), "still down after 3.1 s");
}

/// A Down from Crouched lies in the Down pose with the stance reading Standing; the Revive gets
/// the Player up Standing, through the rise and its firing lock, never back to Crouched.
#[test]
fn a_revive_rises_to_standing_whatever_the_down_was_from() {
    let (mut sim, local, teammate) = co_op();
    sim.world_mut().get_mut::<MoveInput>(teammate).unwrap().crouch = true;
    sim.run(30);
    assert_eq!(sim.world_mut().get::<Locomotion>(teammate).unwrap().stance, Stance::Crouched);
    // Stance input is dead while Down: the resolver sends Standing from here on.
    sim.world_mut().get_mut::<MoveInput>(teammate).unwrap().crouch = false;
    down(&mut sim, teammate);
    sim.run(2);
    let downed = sim.world_mut().get::<Locomotion>(teammate).unwrap().clone();
    assert!(downed.down, "not in the Down pose");
    assert_eq!(downed.stance, Stance::Standing, "the Down didn't reset the stance");

    hold(&mut sim, local, teammate, REVIVE_SECS + 0.1);
    assert!(!in_last_stand(&mut sim, teammate));
    sim.run(2);
    let rising = sim.world_mut().get::<Locomotion>(teammate).unwrap().clone();
    assert!(!rising.down && rising.stance == Stance::Standing, "the Revive didn't get them up Standing");
    assert!(rising.prone_transition(), "the Revive skipped the rise");
    sim.run(sim.frames(LAST_STAND_COLLAPSE_SECS + 0.1));
    let up = sim.world_mut().get::<Locomotion>(teammate).unwrap().clone();
    assert!(!up.changing_stance() && up.stance == Stance::Standing, "the rise didn't end Standing");
}

/// A Self-Revive gets the Player up the same way: Standing, through the rise.
#[test]
fn a_self_revive_rises_to_standing() {
    let (mut sim, _, teammate) = co_op();
    sim.world_mut().get_mut::<MoveInput>(teammate).unwrap().prone = true;
    sim.run(60);
    assert_eq!(sim.world_mut().get::<Locomotion>(teammate).unwrap().stance, Stance::Prone);
    sim.world_mut().get_mut::<MoveInput>(teammate).unwrap().prone = false;
    let world = sim.world_mut();
    let mut entity = world.entity_mut(teammate);
    let stand = LastStand::begin(&mut entity.get_mut::<Loadout>().unwrap());
    entity.insert(stand);
    sim.run(2);
    assert!(sim.world_mut().get::<Locomotion>(teammate).unwrap().down, "not in the Down pose");
    sim.run(sim.frames(LAST_STAND_SECS + 0.1));
    assert!(!in_last_stand(&mut sim, teammate), "no Self-Revive");
    sim.run(sim.frames(LAST_STAND_COLLAPSE_SECS + 0.1));
    let up = sim.world_mut().get::<Locomotion>(teammate).unwrap().clone();
    assert!(!up.down && up.stance == Stance::Standing && !up.changing_stance(), "the Self-Revive didn't end Standing");
}

#[test]
fn a_revive_with_quick_revive_takes_a_second_and_a_half() {
    let (mut sim, local, teammate) = co_op();
    sim.world_mut().get_mut::<Perks>(local).unwrap().0 = vec![Perk::QuickRevive];
    down(&mut sim, teammate);
    hold(&mut sim, local, teammate, 1.4);
    assert!(in_last_stand(&mut sim, teammate), "up after 1.4 s");
    hold(&mut sim, local, teammate, 0.2);
    assert!(!in_last_stand(&mut sim, teammate), "still down after 1.6 s");
}

#[test]
fn only_the_revivers_quick_revive_counts() {
    let (mut sim, local, teammate) = co_op();
    sim.world_mut().get_mut::<Perks>(teammate).unwrap().0 = vec![Perk::QuickRevive];
    down(&mut sim, teammate);
    hold(&mut sim, local, teammate, 2.0);
    assert!(in_last_stand(&mut sim, teammate), "the downed Player's own Quick Revive sped it up");
}

#[test]
fn a_revive_needs_the_reviver_within_two_metres() {
    let (mut sim, local, teammate) = co_op();
    down(&mut sim, teammate);
    set_gap(&mut sim, local, teammate, 2.5);
    hold(&mut sim, local, teammate, 4.0);
    assert!(in_last_stand(&mut sim, teammate), "revived from 2.5 m");
    assert!(sim.world_mut().get::<Reviving>(local).is_none());

    set_gap(&mut sim, local, teammate, 1.5);
    hold(&mut sim, local, teammate, 3.1);
    assert!(!in_last_stand(&mut sim, teammate), "not revived from 1.5 m");
}

#[test]
fn letting_go_resets_the_revive() {
    let (mut sim, local, teammate) = co_op();
    down(&mut sim, teammate);
    hold(&mut sim, local, teammate, 2.5);
    assert!(sim.world_mut().get::<Reviving>(local).is_some());
    sim.run(2);
    assert!(sim.world_mut().get::<Reviving>(local).is_none(), "the hold outlived the key");
    hold(&mut sim, local, teammate, 2.5);
    assert!(in_last_stand(&mut sim, teammate), "the first hold counted");
    hold(&mut sim, local, teammate, 0.7);
    assert!(!in_last_stand(&mut sim, teammate));
}

#[test]
fn a_hit_doesnt_reset_the_revive_and_the_reviver_cant_fire() {
    let (mut sim, local, teammate) = co_op();
    down(&mut sim, teammate);
    hold(&mut sim, local, teammate, 1.0);
    assert!(sim.world_mut().run_system_once(not_reviving).unwrap() == false, "the reviver could fire");
    sim.send(PlayerHit { player: local, damage: 30.0, from: None });
    hold(&mut sim, local, teammate, 1.0);
    assert!(sim.world_mut().get::<Health>(local).unwrap().current < PLAYER_MAX_HEALTH, "the hit never landed");
    hold(&mut sim, local, teammate, 1.1);
    assert!(!in_last_stand(&mut sim, teammate), "the hit reset the Revive");
    assert!(sim.world_mut().run_system_once(not_reviving).unwrap(), "the reviver still can't fire");
}

#[test]
fn a_revive_gives_back_weapons_and_health_and_pays_the_reviver_what_the_down_cost() {
    let (mut sim, local, teammate) = co_op();
    let world = sim.world_mut();
    world.get_mut::<Points>(teammate).unwrap().balance = 3_000;
    world.get_mut::<Points>(local).unwrap().balance = 400;
    let earned = world.get::<Points>(local).unwrap().earned;
    let mut loadout = world.get_mut::<Loadout>(teammate).unwrap();
    loadout.give(WeaponId::Mp40);
    loadout.held_mut().magazine = 7;
    down(&mut sim, teammate);
    let world = sim.world_mut();
    assert_eq!(world.get::<Points>(teammate).unwrap().balance, 2_850);
    hold(&mut sim, local, teammate, 3.2);
    let revived = sim.world_mut().resource::<Seen>().0.clone();
    let world = sim.world_mut();
    let loadout = world.get::<Loadout>(teammate).unwrap();
    assert_eq!((loadout.held().id, loadout.held().magazine), (WeaponId::Mp40, 7));
    let health = world.get::<Health>(teammate).unwrap();
    assert_eq!(health.current, health.max);
    assert_eq!(world.get::<Points>(teammate).unwrap().balance, 2_850, "the Down's cost came back to them");
    let reviver = world.get::<Points>(local).unwrap();
    assert_eq!(reviver.balance, 400 + 150, "the reviver didn't get the 5%");
    assert_eq!(reviver.earned, earned, "the 5% counted as earned");
    assert_eq!(world.get::<RunStats>(local).unwrap().revives, 1);
    assert_eq!(world.get::<RunStats>(teammate).unwrap().revives, 0);
    assert_eq!(revived, [(teammate, local)]);

    // And only once: holding on pays nothing more.
    hold(&mut sim, local, teammate, 4.0);
    assert_eq!(sim.world_mut().get::<Points>(local).unwrap().balance, 550);
}

#[test]
fn a_player_in_last_stand_cant_revive_anyone() {
    let (mut sim, local, teammate) = co_op();
    down(&mut sim, teammate);
    down(&mut sim, local);
    hold(&mut sim, local, teammate, 4.0);
    assert!(in_last_stand(&mut sim, teammate));
    assert!(sim.world_mut().get::<Reviving>(local).is_none());
}

#[test]
fn going_down_mid_revive_drops_the_hold() {
    let (mut sim, local, teammate) = co_op();
    down(&mut sim, teammate);
    hold(&mut sim, local, teammate, 1.0);
    assert!(sim.world_mut().get::<Reviving>(local).is_some());
    down(&mut sim, local);
    assert!(sim.world_mut().get::<Reviving>(local).is_none());
}
