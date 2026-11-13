//! A Player whose way to the Host breaks keeps their Seat all Run, redials with the token the Host
//! gave, and is back in the world at the next Round, caught up on the world they missed.

use super::downs::{coop, knock_down};
use super::tests::{Run, token_of};
use super::*;
use crate::net::away::{Reach, Rejoin};
use crate::net::play::Held;
use crate::net::protocol::Refusal;
use crate::economy::power_up::ActivePowerUps;

fn run(clients: u8) -> Run {
    Run::with(clients, &coop)
}

fn away(run: &mut Run, seat: u8) -> bool {
    let player = run.player(0, seat);
    run.world(0).get::<Disconnected>(player).is_some()
}

fn reach(run: &mut Run, on: usize) -> Reach {
    run.world(on).resource::<Rejoin>().reach
}

#[test]
fn a_dropped_player_is_held_and_never_up() {
    let mut run = run(2);
    run.step(5);
    run.cut(1);
    run.step(2);
    assert!(away(&mut run, 1));
    assert!(!run.world(0).resource::<Roster>().0.iter().any(|&(_, seat)| seat == 1));
    assert!(run.world(0).resource::<Held>().away.contains(&1));
    // Their row is held on every machine that has the Player.
    let player = run.player(0, 1);
    assert!(run.world(0).get::<crate::ui::hud::coop::SeatHeld>(player).is_some());
    assert!(!crate::enemies::target::is_up(false, false, true));
}

#[test]
fn a_client_redials_with_its_token_and_is_back_at_the_next_round_in_its_own_seat() {
    let mut run = run(2);
    run.give_points(1, 1_234);
    let host_copy = run.player(0, 1);
    run.world(0).get_mut::<Perks>(host_copy).unwrap().0.push(Perk::Juggernog);
    let boon = crate::run::gauntlet::sample_boon_ids(1).remove(0);
    show_boons(&mut run.world(0).get_mut::<HeldBoons>(host_copy).unwrap(), &[(boon, 1)]);
    run.step(10);
    run.cut(1);
    run.step(2);
    assert!(matches!(reach(&mut run, 1), Reach::Redialling { .. }));
    run.step(30);
    assert_eq!(reach(&mut run, 1), Reach::Up, "the Host took the Player back");
    assert_eq!(run.world(0).resource::<Roster>().0.iter().filter(|&&(_, seat)| seat == 1).count(), 1);
    // The Seat is theirs again, but they are not in the world until the next Round.
    assert!(away(&mut run, 1));
    run.world(0).write_message(RoundStarted);
    run.step(10);
    assert!(!away(&mut run, 1));
    assert_eq!(run.player(0, 1), host_copy, "the same Player");
    assert_eq!(run.balance(1), 1_234);
    let client = run.player(1, 1);
    assert!(run.world(1).get::<Perks>(client).unwrap().has(Perk::Juggernog));
    assert_eq!(run.world(1).get::<HeldBoons>(client).unwrap().0.len(), 1);
    assert_eq!(
        run.world(1).get::<Loadout>(client).unwrap().weapons().count(),
        run.world(0).get::<Loadout>(host_copy).unwrap().weapons().count()
    );
    assert_eq!(run.world(1).get::<Points>(client).unwrap().balance, 1_234);
}

#[test]
fn a_wrong_or_stale_token_is_refused() {
    let mut run = run(1);
    run.step(5);
    let wrong = run.knock(Some(77));
    assert_eq!(wrong, Some(ToPlayer::Refused(Refusal::NoSeat)));
    let none = run.knock(None);
    assert_eq!(none, Some(ToPlayer::Refused(Refusal::Started)));
    // The token a Player came back by is a new one: the old is stale.
    run.cut(1);
    run.step(40);
    assert_eq!(reach(&mut run, 1), Reach::Up);
    let stale = run.knock(Some(token_of(1)));
    assert_eq!(stale, Some(ToPlayer::Refused(Refusal::NoSeat)));
    assert!(run.world(0).resource::<Roster>().0.iter().any(|&(_, seat)| seat == 1));
}

#[test]
fn a_drop_in_a_last_stand_is_a_bleed_out_that_costs_the_others_nothing() {
    let mut run = run(2);
    run.give_points(2, 1_000);
    run.step(5);
    knock_down(&mut run, 1);
    run.step(10);
    let player = run.player(0, 1);
    assert!(run.world(0).get::<LastStand>(player).is_some());
    run.cut(1);
    run.step(5);
    assert!(run.world(0).get::<LastStand>(player).is_none());
    assert!(run.world(0).get::<AwaitingRespawn>(player).is_some());
    assert_eq!(run.balance(2), 1_000, "no Points were lost");
}

#[test]
fn the_state_a_returning_player_missed_is_told_to_them() {
    let mut run = run(1);
    run.step(5);
    run.cut(1);
    run.step(2);
    for on in 0..2 {
        run.world(on).insert_resource(Generators::for_map(crate::maps::map::MapId::Dam));
    }
    // While they are away: a Plank is torn, Fire Sale starts, a Power-up lies about, one
    // Generator is Held, another Lost and the third bought.
    run.world(0).resource_mut::<Barriers>().tear(BarrierId(3));
    run.world(0).resource_mut::<Generators>().hold(0);
    run.world(0).resource_mut::<Generators>().hold(1);
    run.world(0).resource_mut::<Generators>().lose(1);
    run.world(0).resource_mut::<Generators>().claim(2);
    run.world(0).resource_mut::<Generators>().charge(2, 4.0, true);
    run.world(0).resource_mut::<Generators>().charge(2, 0.0, false);
    run.world(0).resource_mut::<ActivePowerUps>().start(PowerUp::DoublePoints, 0.0);
    run.world(0).spawn(Dropped::new(PowerUp::Carpenter, Vec3::new(40.0, 1.0, 40.0)));
    run.step(10);
    run.step(30);
    assert_eq!(reach(&mut run, 1), Reach::Up);
    run.step(10);
    assert_eq!(run.world(1).resource::<Barriers>().planks(BarrierId(3)), BARRIER_PLANKS - 1);
    assert!(run.world(1).resource::<ActivePowerUps>().active(PowerUp::DoublePoints));
    let generators = run.world(1).resource::<Generators>();
    assert_eq!(generators.states()[0], GeneratorState::Held);
    assert_eq!(generators.states()[1], GeneratorState::Lost);
    assert!((generators.states()[2].fraction() - 0.2).abs() < 0.01, "{:?}", generators.states()[2]);
    let world = run.world(1);
    let mut query = world.query::<&Dropped>();
    assert_eq!(query.iter(world).count(), 1);
}
