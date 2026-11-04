//! Two Players in one world on the real Map, with no netcode: one is the `LocalPlayer`, the other
//! is driven only through `MoveInput` and Player-tagged messages, as a teammate will be.

use super::*;
use crate::barrier::{Barrier, Barriers, PlankRebuilt, rebuild_barriers};
use crate::equipment::Equipment;
use crate::power_up::{Dropped, GivePowerUp, PowerUp, PowerUpTaken};
use crate::round::Round;
use crate::map::BarrierId;
use crate::ammo_mod::{AmmoMods, AmmoModHit};
use crate::explosive::{Explosion, explode, load_assets};
use crate::settings::Settings;
use bevy::ecs::system::RunSystemOnce;
use crate::zombie::Gibbed;
use crate::buy::{Buyable, BuyRefused, CantAfford, HoldPurchased, Purchased, UseRequest, resolve_use};
use crate::points::{Points, PointsPlugin};
use crate::zombie::headless::Sim;
use crate::zombie::{Attack, DamageZombie, HitZone, Zombie};

/// The Map, the Zombie systems, Points and the buy handler, with Seat 0's `LocalPlayer` and
/// Seat 1's teammate in the Start Room. Returns the Sim and the two Players.
pub(crate) fn two_players() -> (Sim, Entity, Entity) {
    two_players_with(|_| {})
}

/// The same, with `configure` adding what a module under test needs to the `App`.
pub(crate) fn two_players_with(configure: impl FnOnce(&mut App)) -> (Sim, Entity, Entity) {
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
            .add_message::<Teleport>()
            .init_resource::<PendingTeleports>()
            .add_systems(PostUpdate, queue_teleports)
            .add_systems(FixedUpdate, (teleport_player, move_player).chain());
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

mod hud;
mod respawn;
mod revive;

/// Carries `player` to stand with their feet at `feet`, the way a Teleport does.
pub(crate) fn place(sim: &mut Sim, player: Entity, feet: Vec3) {
    sim.send(Teleport { player, feet, facing: None });
    sim.run(4);
}

pub(crate) fn feet(sim: &mut Sim, player: Entity) -> Vec3 {
    let half_height = PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0;
    sim.world_mut().get::<Transform>(player).unwrap().translation - Vec3::Y * half_height
}

pub(crate) fn earned(sim: &mut Sim, player: Entity) -> u64 {
    sim.world_mut().get::<Points>(player).unwrap().earned
}

pub(crate) fn balance(sim: &mut Sim, player: Entity) -> u64 {
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

/// Starts a Last Stand for `player`.
fn down(sim: &mut Sim, player: Entity) {
    let world = sim.world_mut();
    let mut entity = world.entity_mut(player);
    let last_stand = crate::last_stand::LastStand::begin(&mut entity.get_mut::<Loadout>().unwrap());
    entity.insert(last_stand);
}

fn set_frags(sim: &mut Sim, player: Entity, frags: u32) {
    sim.world_mut().get_mut::<Equipment>(player).unwrap().frags = frags;
}

fn frags(sim: &mut Sim, player: Entity) -> u32 {
    sim.world_mut().get::<Equipment>(player).unwrap().frags
}

/// Adds the Power-up pickup to a fresh pair of Players.
fn power_up_players() -> (Sim, Entity, Entity) {
    let (mut sim, local, teammate) = two_players();
    sim.app
        .add_message::<PowerUpTaken>()
        .add_message::<GivePowerUp>()
        .add_systems(Update, crate::power_up::pick_up);
    (sim, local, teammate)
}

fn drop_at(sim: &mut Sim, power_up: PowerUp, player: Entity) {
    let at = sim.world_mut().get::<Transform>(player).unwrap().translation;
    sim.world_mut().spawn(Dropped::new(power_up, at));
}

#[test]
fn a_nuke_taken_by_one_player_pays_both() {
    let (mut sim, local, teammate) = power_up_players();
    let (local_before, teammate_before) = (balance(&mut sim, local), balance(&mut sim, teammate));
    drop_at(&mut sim, PowerUp::Nuke, local);
    sim.run(2);
    assert_eq!(balance(&mut sim, local), local_before + NUKE_POINTS);
    assert_eq!(balance(&mut sim, teammate), teammate_before + NUKE_POINTS);
    let world = sim.world_mut();
    assert_eq!(world.query::<&Dropped>().iter(world).count(), 0, "it is taken once");
}

#[test]
fn the_teammate_can_take_a_power_up_too() {
    let (mut sim, local, teammate) = power_up_players();
    let (local_before, teammate_before) = (balance(&mut sim, local), balance(&mut sim, teammate));
    drop_at(&mut sim, PowerUp::Carpenter, teammate);
    sim.run(2);
    assert_eq!(balance(&mut sim, local), local_before + CARPENTER_POINTS);
    assert_eq!(balance(&mut sim, teammate), teammate_before + CARPENTER_POINTS);
}

#[test]
fn max_ammo_skips_a_player_in_a_last_stand() {
    let (mut sim, local, teammate) = power_up_players();
    set_frags(&mut sim, local, 0);
    set_frags(&mut sim, teammate, 0);
    down(&mut sim, teammate);
    sim.send(GivePowerUp(PowerUp::MaxAmmo, None));
    sim.run(2);
    assert!(frags(&mut sim, local) > 0, "the standing Player was refilled");
    assert_eq!(frags(&mut sim, teammate), 0, "the Player in a Last Stand was not");
}

#[test]
fn the_flat_bonuses_skip_a_player_in_a_last_stand() {
    let (mut sim, local, teammate) = power_up_players();
    let (local_before, teammate_before) = (balance(&mut sim, local), balance(&mut sim, teammate));
    down(&mut sim, teammate);
    sim.send(GivePowerUp(PowerUp::Nuke, None));
    sim.run(2);
    assert_eq!(balance(&mut sim, local), local_before + NUKE_POINTS);
    assert_eq!(balance(&mut sim, teammate), teammate_before);
}

#[test]
fn double_points_doubles_every_players_awards() {
    let (mut sim, local, teammate) = power_up_players();
    sim.send(GivePowerUp(PowerUp::DoublePoints, None));
    sim.run(2);
    for player in [local, teammate] {
        assert!(sim.world_mut().get::<Points>(player).unwrap().double);
    }
}

/// Plank Points `player` earns putting `planks` Planks back in Round `round`.
fn rebuild_for(sim: &mut Sim, player: Entity, round: u32, planks: u32) -> u64 {
    let before = earned(sim, player);
    sim.world_mut().insert_resource(Round::in_progress(round));
    for _ in 0..planks {
        let world = sim.world_mut();
        let barrier = BarrierId(0);
        let existing = world.query::<(Entity, &Barrier)>().iter(world).next().map(|(entity, _)| entity);
        let buyable = existing.unwrap_or_else(|| world.spawn(Barrier(barrier)).id());
        world.resource_mut::<Barriers>().smash(barrier);
        sim.send(UseRequest { player, buyable, hold: false });
        sim.run(1);
    }
    earned(sim, player) - before
}

fn plank_players() -> (Sim, Entity, Entity) {
    let (mut sim, local, teammate) = two_players();
    sim.app.add_message::<PlankRebuilt>().add_systems(Update, rebuild_barriers);
    (sim, local, teammate)
}

#[test]
fn plank_points_are_capped_per_player_per_round() {
    let (mut sim, local, teammate) = plank_players();
    // Round 2 caps a Player at 100, checked before each award: nine Planks, then none.
    assert_eq!(rebuild_for(&mut sim, local, 2, 20), 90);
    assert_eq!(rebuild_for(&mut sim, teammate, 2, 20), 90, "the teammate has their own cap");
    assert_eq!(rebuild_for(&mut sim, local, 3, 20), 140, "a new Round resets it");
}

#[test]
fn the_solo_plank_cap_is_unchanged() {
    // One Player, as in a solo Run: Round 1 caps them at 50, so four Planks of ten.
    let (mut sim, local, _) = plank_players();
    assert_eq!(rebuild_for(&mut sim, local, 1, 20), 40);
}

/// A blast at the Players' feet, with a Zombie close by, thrown by `thrower`. Returns the
/// Players it hurt.
fn blast(sim: &mut Sim, thrower: Entity, hurts_player: bool) -> Vec<Entity> {
    // At Seat 1's spot, which has a clear view of the floor around it.
    let local_at = sim.world_mut().query::<(&Seat, &Transform)>().iter(sim.world_mut()).find(|(seat, _)| seat.0 == 1).unwrap().1.translation;
    // Both Players stand together, so the blast reaches either.
    for player in sim.world_mut().query_filtered::<Entity, With<Player>>().iter(sim.world_mut()).collect::<Vec<_>>() {
        sim.world_mut().get_mut::<Transform>(player).unwrap().translation = local_at;
    }
    sim.spawn(local_at - Vec3::Y * 0.8 + Vec3::X * 1.5, None);
    sim.run(2);
    sim.send(Explosion {
        at: local_at,
        thrower: Some(thrower),
        splash: FRAG_SPLASH,
        hurts_player,
        ammo_mods: AmmoMods::NONE,
        crawlers: false,
    });
    let mut hurt = Vec::new();
    for _ in 0..4 {
        sim.run(1);
        hurt.extend(sim.world_mut().resource_mut::<Messages<PlayerHit>>().drain().map(|hit| hit.player));
    }
    hurt
}

fn explosive_players() -> (Sim, Entity, Entity) {
    let (mut sim, local, teammate) = two_players();
    sim.app
        .add_message::<Explosion>()
        .add_message::<AmmoModHit>()
        .add_message::<Gibbed>()
        .init_resource::<Settings>()
        .add_systems(Update, explode);
    sim.world_mut().run_system_once(load_assets).unwrap();
    (sim, local, teammate)
}

#[test]
fn a_players_own_blast_hurts_only_them_and_pays_only_them() {
    let (mut sim, local, teammate) = explosive_players();
    let hurt = blast(&mut sim, teammate, true);
    assert_eq!(hurt, [teammate], "the local Player standing in the blast was hurt");
    assert!(earned(&mut sim, teammate) > 0, "the thrower wasn't paid for the hit");
    assert_eq!(earned(&mut sim, local), 0);
}

#[test]
fn a_claymore_hurts_no_player_and_pays_its_owner() {
    let (mut sim, local, teammate) = explosive_players();
    let hurt = blast(&mut sim, local, false);
    assert!(hurt.is_empty(), "a Claymore hurt {hurt:?}");
    assert!(earned(&mut sim, local) > 0, "the owner wasn't paid");
    assert_eq!(earned(&mut sim, teammate), 0);
}
