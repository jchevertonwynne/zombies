//! The Fuel Lines over the network: a client's Valve request shuts the line on both machines, a client sees every phase change the Host
//! makes, and a Player who comes back during a Shut is told the phase and how far into it the line is.

use super::tests::Run;
use super::*;
use crate::maps::fire::{FirePlugin, LitMask, Valve};
use crate::maps::map::MapId;
use crate::players::player::Seat;
use crate::players::respawn::SeatReturned;

fn refinery(clients: u8) -> Run {
    Run::on(MapId::Refinery, clients, &|app| {
        app.add_plugins(FirePlugin);
    })
}

fn phase(run: &mut Run, on: usize, line: u8) -> LinePhase {
    run.world(on).resource::<FuelLines>().phase(FuelLineId(line))
}

fn host_shows(run: &mut Run, line: u8, phase: LinePhase) {
    run.world(0).resource_mut::<FuelLines>().show(FuelLineId(line), phase);
}

fn valve_of(run: &mut Run, on: usize, line: u8) -> Entity {
    run.find::<Valve>(on, |valve| valve.line == FuelLineId(line))
}

#[test]
fn a_clients_valve_request_shuts_the_line_on_both_machines() {
    let mut run = refinery(1);
    run.give_points(1, 1000);
    for on in 0..2 {
        assert_eq!(run.world(on).resource::<FuelLines>().count(), 6, "game {on} builds the six lines from the Layout");
        assert!((0..6).all(|line| phase(&mut run, on, line) == LinePhase::Lit), "game {on} starts with every line Lit");
    }
    let valve = valve_of(&mut run, 1, 3);
    let id = *run.world(1).get::<NetId>(valve).unwrap();
    run.use_buyable(1, valve, false);
    run.step(20);
    for on in 0..2 {
        assert!(matches!(phase(&mut run, on, 3), LinePhase::Shut { .. }), "game {on} has the Flare Line {:?}", phase(&mut run, on, 3));
        assert!((0..6).filter(|&line| line != 3).all(|line| phase(&mut run, on, line) == LinePhase::Lit), "game {on}: no other line");
    }
    assert_eq!(run.world(0).resource::<FuelLines>().closed(), LitMask(0b11_0111), "the Host routes Zombies round the five that are left");
    assert_eq!(run.balance(1), 1000 - 250, "the Host charged the buyer the Valve's 250");
    assert_eq!(run.seen(1).bought, 1, "the buyer is told the buy went through");
    // Every game names the Valve alike.
    for on in 0..2 {
        assert!(run.world(on).resource::<NetIds>().entity(id.0).is_some_and(|entity| run.world(on).get::<Valve>(entity).is_some()), "game {on}");
    }
    // A second use of the same line's Valve at once (it is Shut under 10 s) charges nothing.
    run.use_buyable(1, valve, false);
    run.step(20);
    assert_eq!(run.balance(1), 750, "a Shut line is not paid for twice");
    assert!(matches!(phase(&mut run, 0, 3), LinePhase::Shut { .. }));
}

#[test]
fn a_client_sees_every_fuel_line_phase_change() {
    let mut run = refinery(1);
    run.step(10);
    // The Host's clock runs a Shut line into its Flare-up and back to Lit; the client is told each.
    run.world(0).resource_mut::<FuelLines>().use_valve(FuelLineId(0), false).unwrap();
    run.step(10);
    assert!(matches!(phase(&mut run, 1, 0), LinePhase::Shut { .. }), "{:?}", phase(&mut run, 1, 0));
    host_shows(&mut run, 0, LinePhase::Shut { elapsed: SHUT_SECS - 0.05 });
    run.step(10);
    assert!(matches!(phase(&mut run, 0, 0), LinePhase::FlareUp { .. }));
    assert!(matches!(phase(&mut run, 1, 0), LinePhase::FlareUp { .. }), "{:?}", phase(&mut run, 1, 0));
    assert_eq!(run.world(1).resource::<FuelLines>().closed(), LitMask::first(6), "closed to Zombies from the start of the Flare-up");
    assert_eq!(run.world(1).resource::<FuelLines>().burning(), LitMask::first(6).without(FuelLineId(0)), "but not yet burning");
    host_shows(&mut run, 0, LinePhase::FlareUp { elapsed: FLARE_UP_SECS - 0.05 });
    run.step(10);
    assert_eq!(phase(&mut run, 0, 0), LinePhase::Lit);
    assert_eq!(phase(&mut run, 1, 0), LinePhase::Lit);
    // The seconds run on the client only to show them: it stays in the phase until the Host says.
    run.world(0).resource_mut::<FuelLines>().use_valve(FuelLineId(1), false).unwrap();
    run.step(10);
    run.step((2.0 * FIXED_HZ as f32) as u32);
    let Some(LinePhase::Shut { elapsed }) = Some(phase(&mut run, 1, 1)) else { panic!("{:?}", phase(&mut run, 1, 1)) };
    assert!(elapsed > 1.5 && elapsed < SHUT_SECS, "{elapsed}");
}

#[test]
fn a_late_joiner_during_a_shut_gets_the_phase_and_elapsed() {
    let mut run = refinery(1);
    run.step(10);
    host_shows(&mut run, 2, LinePhase::Shut { elapsed: 20.0 });
    host_shows(&mut run, 4, LinePhase::FlareUp { elapsed: 1.0 });
    run.step(10);
    // The client forgets, as one that was away would have.
    *run.world(1).resource_mut::<FuelLines>() = FuelLines::start(6);
    run.world(0).write_message(SeatReturned { seat: Seat(1) });
    run.step(10);
    let Some(LinePhase::Shut { elapsed }) = Some(phase(&mut run, 1, 2)) else { panic!("{:?}", phase(&mut run, 1, 2)) };
    assert!((20.0..21.0).contains(&elapsed), "with the seconds into it: {elapsed}");
    assert!(matches!(phase(&mut run, 1, 4), LinePhase::FlareUp { .. }), "{:?}", phase(&mut run, 1, 4));
    for line in [0, 1, 3, 5] {
        assert_eq!(phase(&mut run, 1, line), LinePhase::Lit, "line {line}");
    }
}
