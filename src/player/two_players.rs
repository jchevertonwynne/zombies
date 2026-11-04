//! Two Players in one world on the real Map, with no netcode: one is the `LocalPlayer`, the other
//! is driven only through `MoveInput` and Player-tagged messages, as a teammate will be.

use super::*;
use crate::buy::{Buyable, BuyRefused, CantAfford, HoldPurchased, Purchased, UseRequest, resolve_use};
use crate::points::{Points, PointsPlugin};
use crate::zombie::headless::Sim;
use crate::zombie::{Attack, DamageZombie, HitZone, Zombie};

/// The Map, the Zombie systems, Points and the buy handler, with Seat 0's `LocalPlayer` and
/// Seat 1's teammate in the Start Room. Returns the Sim and the two Players.
fn two_players() -> (Sim, Entity, Entity) {
    two_players_with(|_| {})
}

/// The same, with `configure` adding whatever else the test needs.
fn two_players_with(configure: impl FnOnce(&mut App)) -> (Sim, Entity, Entity) {
    let mut sim = Sim::new(60.0, |app| {
        configure(app);
        app.add_plugins(PointsPlugin)
            .init_resource::<RunModifiers>()
            .add_message::<UseRequest>()
            .add_message::<Purchased>()
            .add_message::<HoldPurchased>()
            .add_message::<CantAfford>()
            .add_message::<BuyRefused>()
            .add_systems(Update, resolve_use)
            .add_systems(FixedUpdate, move_player);
    });
    let world = sim.world_mut();
    // The Sim's stand-in Player gives way to two real ones.
    let stand_in = world.query_filtered::<Entity, With<Player>>().single(world).unwrap();
    world.entity_mut(stand_in).despawn();
    let local = world.spawn((player_body(Seat(0), "Player", &[]), LocalPlayer)).id();
    let teammate = world.spawn(player_body(Seat(1), "Player 2", &[])).id();
    sim.run(30);
    (sim, local, teammate)
}

mod revive;

fn feet(sim: &mut Sim, player: Entity) -> Vec3 {
    let half_height = PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0;
    sim.world_mut().get::<Transform>(player).unwrap().translation - Vec3::Y * half_height
}

fn earned(sim: &mut Sim, player: Entity) -> u64 {
    sim.world_mut().get::<Points>(player).unwrap().earned
}

fn balance(sim: &mut Sim, player: Entity) -> u64 {
    sim.world_mut().get::<Points>(player).unwrap().balance
}

#[test]
fn a_run_holds_two_players_and_only_one_is_local() {
    let (mut sim, local, teammate) = two_players();
    let world = sim.world_mut();
    assert_eq!(world.query_filtered::<(), With<Player>>().iter(world).count(), 2);
    let locals: Vec<Entity> = world.query_filtered::<Entity, With<LocalPlayer>>().iter(world).collect();
    assert_eq!(locals, [local]);
    assert!(world.get::<LocalPlayer>(teammate).is_none());
    assert_eq!(world.query::<&PlayerCamera>().iter(world).count(), 0, "the teammate has no camera");
    assert_eq!(world.get::<Seat>(teammate), Some(&Seat(1)));
}

#[test]
fn the_teammate_walks_on_move_input_alone() {
    let (mut sim, local, teammate) = two_players();
    let (local_from, from) = (feet(&mut sim, local), feet(&mut sim, teammate));
    sim.world_mut().get_mut::<MoveInput>(teammate).unwrap().axes = Vec2::Y;
    sim.run(sim.frames(1.0));
    let to = feet(&mut sim, teammate);
    // Facing -Z at the start, so forward is north, along the Seat's own column.
    assert!(from.z - to.z > 2.0, "walked only {} m", from.z - to.z);
    assert!((to.x - from.x).abs() < 0.2, "drifted sideways to {}", to.x);
    assert!(feet(&mut sim, local).distance(local_from) < 0.1, "the local Player moved too");
}

/// A Zombie in the Start Room goes after the Player nearer to it and swipes them, never the other.
#[test]
fn a_zombie_chases_the_nearer_of_the_two() {
    let (mut sim, local, teammate) = two_players();
    // The teammate walks west, away from the local Player.
    sim.world_mut().get_mut::<MoveInput>(teammate).unwrap().axes = Vec2::NEG_X;
    sim.run(sim.frames(1.5));
    sim.world_mut().get_mut::<MoveInput>(teammate).unwrap().axes = Vec2::ZERO;
    assert!(feet(&mut sim, teammate).x < -6.0, "the teammate is at {}", feet(&mut sim, teammate));
    sim.spawn(Vec3::new(-12.0, 0.0, 22.0), None);
    let mut hit = Vec::new();
    for _ in 0..sim.frames(20.0) {
        sim.run(1);
        hit.extend(sim.world_mut().resource_mut::<Messages<PlayerHit>>().drain().map(|hit| hit.player));
    }
    assert!(!hit.is_empty(), "the Zombie never landed a swipe");
    assert!(hit.iter().all(|&player| player == teammate), "it swiped the Player who was farther");
    assert_ne!(hit[0], local);
}

/// A kill pays the Player whose hit made it, and the other Player gets nothing.
#[test]
fn a_kill_pays_the_player_who_landed_it() {
    let (mut sim, local, teammate) = two_players();
    let kill = |sim: &mut Sim, by: Entity| {
        let zombie = sim.spawn(Vec3::new(0.0, 0.0, 0.0), None);
        sim.send(DamageZombie { player: Some(by), zombie, damage: 1e6, zone: HitZone::Head, attack: Attack::Bullet });
        sim.run(5);
    };
    kill(&mut sim, teammate);
    assert!(earned(&mut sim, teammate) >= POINTS_HEADSHOT_KILL, "the teammate wasn't paid");
    assert_eq!(earned(&mut sim, local), 0, "the local Player was paid for the teammate's kill");

    let teammate_earned = earned(&mut sim, teammate);
    kill(&mut sim, local);
    assert!(earned(&mut sim, local) >= POINTS_HEADSHOT_KILL, "the local Player wasn't paid");
    assert_eq!(earned(&mut sim, teammate), teammate_earned, "the teammate was paid for the local Player's kill");
    let world = sim.world_mut();
    assert_eq!(world.query_filtered::<(), (With<Zombie>, Without<crate::zombie::Dying>)>().iter(world).count(), 0);
}

/// A `UseRequest` charges the Player who made it, whoever is local.
#[test]
fn a_use_request_charges_the_player_who_made_it() {
    let (mut sim, local, teammate) = two_players();
    let (local_before, teammate_before) = (balance(&mut sim, local), balance(&mut sim, teammate));
    assert!(teammate_before >= 250);
    let door = sim
        .world_mut()
        .spawn(Buyable { use_point: Vec3::ZERO, prompt: String::new(), price: Some(250), range: USE_RANGE })
        .id();
    sim.send(UseRequest { player: teammate, buyable: door, hold: false });
    sim.run(1);
    assert_eq!(balance(&mut sim, teammate), teammate_before - 250);
    assert_eq!(balance(&mut sim, local), local_before, "the local Player paid for the teammate's buy");
    let bought: Vec<_> = sim.world_mut().resource_mut::<Messages<Purchased>>().drain().map(|p| p.player).collect();
    assert_eq!(bought, [teammate]);
}
