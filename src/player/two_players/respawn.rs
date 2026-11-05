//! Co-op Bleed Out, Respawn and Game Over between two Players in one world.

use super::revive::{co_op_with, down, hold, in_last_stand, set_gap};
use super::*;
use crate::arsenal::WeaponId;
use crate::equipment::Equipment;
use crate::perk::{Perk, Perks};
use crate::respawn::{AwaitingRespawn, RespawnPlugin};
use crate::round::{Round, RoundStarted};
use crate::state::{GameState, RunType};
use crate::target::Players;
use crate::weapon::Loadout;
use crate::zone::{ActiveZones, PlayerZone, ZoneId, update_active_zones};
use bevy::ecs::system::RunSystemOnce;

/// Two Players in a Co-op Run, a metre apart, in Round `round`.
fn co_op(round: u32) -> (Sim, Entity, Entity) {
    let (mut sim, local, teammate) = co_op_with(|app| {
        app.add_plugins(RespawnPlugin)
            .add_message::<RoundStarted>()
            .insert_resource(Round::in_progress(round));
    });
    set_gap(&mut sim, local, teammate, 1.0);
    (sim, local, teammate)
}

/// Downs `player` and lets the whole Co-op Last Stand run out.
fn bleed_out(sim: &mut Sim, player: Entity) {
    down(sim, player);
    sim.run(sim.frames(LAST_STAND_COOP_SECS + 1.0));
}

fn waiting(sim: &mut Sim, player: Entity) -> bool {
    sim.world_mut().get::<AwaitingRespawn>(player).is_some()
}

fn start_round(sim: &mut Sim) {
    sim.send(RoundStarted);
    sim.run(2);
}

/// Drops `player` to zero Health for one frame. The harness can't run on past a Game Over (its
/// Pause state goes with Playing), so a test reads the Game Over still pending in that frame.
fn knock_down(sim: &mut Sim, player: Entity) {
    sim.world_mut().get_mut::<Health>(player).unwrap().current = 0.0;
    sim.run(1);
}

fn game_over(sim: &mut Sim) -> bool {
    matches!(sim.world_mut().resource::<NextState<GameState>>(), NextState::Pending(GameState::GameOver))
}

fn is_up(sim: &mut Sim, player: Entity) -> bool {
    sim.world_mut().run_system_once(move |players: Players| players.get(player).unwrap().up).unwrap()
}

#[test]
fn a_bleed_out_takes_ten_percent_from_everyone_else_and_nothing_from_the_player() {
    let (mut sim, local, teammate) = co_op(3);
    sim.world_mut().get_mut::<Points>(local).unwrap().balance = 2_000;
    sim.world_mut().get_mut::<Points>(teammate).unwrap().balance = 1_000;
    bleed_out(&mut sim, teammate);
    assert!(waiting(&mut sim, teammate), "never started waiting");
    assert!(!in_last_stand(&mut sim, teammate));
    assert_eq!(balance(&mut sim, teammate), 950, "the Player lost more than the Down's cost");
    assert_eq!(balance(&mut sim, local), 1_800);
}

#[test]
fn a_revive_in_time_is_no_bleed_out() {
    let (mut sim, local, teammate) = co_op(3);
    sim.world_mut().get_mut::<Points>(local).unwrap().balance = 2_000;
    sim.world_mut().get_mut::<Points>(teammate).unwrap().balance = 500;
    down(&mut sim, teammate);
    hold(&mut sim, local, teammate, 3.2);
    sim.run(sim.frames(LAST_STAND_COOP_SECS));
    assert!(!waiting(&mut sim, teammate));
    assert_eq!(balance(&mut sim, local), 2_000 + 25);
}

#[test]
fn a_player_waiting_to_respawn_has_no_body_isnt_targeted_and_holds_no_zone_open() {
    let (mut sim, local, teammate) = co_op(3);
    bleed_out(&mut sim, teammate);
    assert!(sim.world_mut().get::<ColliderDisabled>(teammate).is_some(), "still has a body");
    assert!(is_up(&mut sim, local));
    assert!(!is_up(&mut sim, teammate), "still a target");
    assert_eq!(sim.world_mut().run_system_once(|players: Players| players.up_count()).unwrap(), 1);

    // They stand in the Yard, far from the local Player in the Start Room.
    let world = sim.world_mut();
    world.get_mut::<PlayerZone>(teammate).unwrap().0 = ZoneId::Yard;
    world.get_mut::<PlayerZone>(local).unwrap().0 = ZoneId::StartRoom;
    world.insert_resource(ActiveZones(Default::default()));
    world.run_system_once(update_active_zones).unwrap();
    assert!(!world.resource::<ActiveZones>().0.contains(ZoneId::Yard), "a waiting Player holds the Yard open");
    assert!(world.resource::<ActiveZones>().0.contains(ZoneId::StartRoom));
}

#[test]
fn a_player_waiting_to_respawn_cant_be_hurt() {
    let (mut sim, _, teammate) = co_op(3);
    bleed_out(&mut sim, teammate);
    sim.send(PlayerHit { player: teammate, damage: 50.0 });
    sim.run(2);
    assert_eq!(sim.world_mut().get::<Health>(teammate).unwrap().current, 0.0);
}

#[test]
fn the_local_player_waiting_to_respawn_watches_a_teammate_who_is_up() {
    let (mut sim, local, teammate) = co_op(3);
    bleed_out(&mut sim, local);
    assert!(waiting(&mut sim, local));
    sim.run(5);
    let world = sim.world_mut();
    let watching = world.get::<Transform>(local).unwrap().translation;
    let followed = world.get::<Transform>(teammate).unwrap().translation;
    let distance = watching.distance(followed);
    let expected = (SPECTATE_BEHIND_DISTANCE.powi(2) + SPECTATE_LIFT.powi(2)).sqrt();
    assert!((distance - expected).abs() < 0.1, "watching from {distance} m, not {expected}");
}

#[test]
fn a_player_respawns_at_the_next_round_beside_a_teammate_with_the_starting_loadout() {
    let (mut sim, local, teammate) = co_op(3);
    let world = sim.world_mut();
    world.get_mut::<Perks>(teammate).unwrap().0 = vec![Perk::Juggernog];
    world.get_mut::<Loadout>(teammate).unwrap().give(WeaponId::Mp40);
    world.get_mut::<Equipment>(teammate).unwrap().claymores = 2;
    world.get_mut::<Points>(teammate).unwrap().balance = 200;
    // They go down Crouched; a Respawn places them Standing.
    sim.world_mut().get_mut::<MoveInput>(teammate).unwrap().crouch = true;
    sim.run(10);
    assert_eq!(sim.world_mut().get::<Locomotion>(teammate).unwrap().stance, Stance::Crouched);
    sim.world_mut().get_mut::<MoveInput>(teammate).unwrap().crouch = false;
    bleed_out(&mut sim, teammate);
    // Nothing happens until the Round starts.
    sim.run(sim.frames(3.0));
    assert!(waiting(&mut sim, teammate));

    start_round(&mut sim);
    sim.run(5);
    let world = sim.world_mut();
    assert!(world.get::<AwaitingRespawn>(teammate).is_none(), "still waiting");
    assert!(world.get::<ColliderDisabled>(teammate).is_none(), "no body");
    assert!(world.get::<Perks>(teammate).unwrap().0.is_empty());
    let loadout = world.get::<Loadout>(teammate).unwrap();
    assert_eq!(loadout.held().id, STARTING_WEAPON);
    assert!(loadout.slot(1).is_none(), "kept a Weapon");
    assert_eq!(world.get::<Equipment>(teammate).unwrap().claymores, 0);
    let health = world.get::<Health>(teammate).unwrap();
    assert_eq!(health.current, health.max);
    assert_eq!(world.get::<Points>(teammate).unwrap().balance, 190, "no floor before Round 7");
    let at = world.get::<Transform>(teammate).unwrap().translation;
    let beside = world.get::<Transform>(local).unwrap().translation;
    // Beside them, a little closer than the offset if a wall pushed the body in.
    assert!((0.5..=RESPAWN_BESIDE_DISTANCE + 0.1).contains(&at.distance(beside)), "{} m from the teammate", at.distance(beside));
    assert!(is_up(&mut sim, teammate));
    assert_eq!(
        sim.world_mut().get::<Locomotion>(teammate).unwrap().stance,
        Stance::Standing,
        "the Respawn didn't place them Standing"
    );
}

#[test]
fn a_respawn_from_round_seven_raises_the_points_to_the_floor() {
    let (mut sim, _, teammate) = co_op(7);
    sim.world_mut().get_mut::<Points>(teammate).unwrap().balance = 400;
    bleed_out(&mut sim, teammate);
    start_round(&mut sim);
    assert_eq!(balance(&mut sim, teammate), RESPAWN_POINTS_FLOOR);

    let (mut sim, _, teammate) = co_op(12);
    sim.world_mut().get_mut::<Points>(teammate).unwrap().balance = 9_000;
    bleed_out(&mut sim, teammate);
    start_round(&mut sim);
    assert_eq!(balance(&mut sim, teammate), 8_550, "the floor took Points away");
}

/// With the teammate standing against a wall on their right, the Player comes back on another
/// side, not inside the wall.
#[test]
fn a_respawn_beside_a_teammate_against_a_wall_picks_a_spot_that_fits() {
    let (mut sim, local, teammate) = co_op(3);
    // Walk the teammate east until the wall stops them (they face north, so east is their right).
    sim.world_mut().get_mut::<MoveInput>(teammate).unwrap().axes = Vec2::X;
    sim.run(sim.frames(10.0));
    sim.world_mut().get_mut::<MoveInput>(teammate).unwrap().axes = Vec2::ZERO;
    sim.run(5);
    bleed_out(&mut sim, local);
    start_round(&mut sim);
    sim.run(3);
    let world = sim.world_mut();
    let at = world.get::<Transform>(local).unwrap().translation;
    let beside = world.get::<Transform>(teammate).unwrap().translation;
    let right = beside + Vec3::X * RESPAWN_BESIDE_DISTANCE;
    assert!(at.distance(right) > 0.5, "came back on the wall side, at {at}");
    assert!((at.distance(beside) - RESPAWN_BESIDE_DISTANCE).abs() < 0.2, "{} m away, so not a free side spot", at.distance(beside));
}

#[test]
fn the_run_goes_on_while_one_player_is_up() {
    let (mut sim, _, teammate) = co_op(3);
    bleed_out(&mut sim, teammate);
    sim.run(sim.frames(5.0));
    assert!(!game_over(&mut sim));
    assert_eq!(*sim.world_mut().resource::<State<GameState>>().get(), GameState::Playing);
}

#[test]
fn the_run_ends_the_moment_no_player_is_up_whatever_last_stand_time_is_left() {
    let (mut sim, local, teammate) = co_op(3);
    down(&mut sim, teammate);
    sim.run(sim.frames(2.0));
    assert!(!game_over(&mut sim), "one Player in Last Stand and one up");
    knock_down(&mut sim, local);
    assert!(in_last_stand(&mut sim, local) && in_last_stand(&mut sim, teammate));
    assert!(game_over(&mut sim));
}

#[test]
fn the_run_ends_when_the_last_player_up_goes_down_after_a_bleed_out() {
    let (mut sim, local, teammate) = co_op(3);
    bleed_out(&mut sim, teammate);
    knock_down(&mut sim, local);
    assert!(game_over(&mut sim));
}

#[test]
fn a_co_op_run_down_to_one_player_keeps_co_op_rules() {
    let (mut sim, local, teammate) = co_op(3);
    sim.world_mut().entity_mut(teammate).despawn();
    assert_eq!(*sim.world_mut().resource::<RunType>(), RunType::Coop);
    knock_down(&mut sim, local);
    // Still a Co-op Down: a Last Stand with no Self-Revive, and Game Over because nobody is up.
    assert!(in_last_stand(&mut sim, local));
    assert!(game_over(&mut sim));
}
