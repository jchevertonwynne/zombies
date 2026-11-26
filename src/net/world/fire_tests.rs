//! The Fuel Lines over the network: a client's Valve request shuts the line on both machines, a client sees every phase change the Host
//! makes, and a Player who comes back during a Shut is told the phase and how far into it the line is.

use bevy::ecs::system::RunSystemOnce;

use super::tests::Run;
use super::*;
use crate::maps::fire::{FirePlugin, LitMask, Valve};
use crate::players::player::Health;
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

// ---- What a Lit Blaze does to a client ----

fn ticks(secs: f32) -> u32 {
    (secs * FIXED_HZ as f32).ceil() as u32
}

/// Puts the Player in `seat` with their feet at `feet` on every game.
fn put(run: &mut Run, seat: u8, feet: Vec3) {
    use crate::players::player::Stance;
    let at = feet + Vec3::Y * Stance::Standing.half_height();
    for on in 0..2 {
        let player = run.player(on, seat);
        run.world(on).get_mut::<Transform>(player).unwrap().translation = at;
        run.world(on).entity_mut(player).insert(avian3d::prelude::Position(at));
    }
}

fn feet_on(run: &mut Run, on: usize, seat: u8) -> Vec3 {
    let player = run.player(on, seat);
    let world = run.world(on);
    world.get::<Transform>(player).unwrap().translation - Vec3::Y * crate::players::player::Stance::Standing.half_height()
}

fn open_every_zone(run: &mut Run) {
    run.world(0).resource_mut::<crate::maps::zone::ActiveZones>().0 = crate::maps::zone::ZoneSet::from_iter(crate::maps::zone::ZoneId::ALL);
}

fn middle_of_line(line: u8) -> Vec3 {
    MapId::Refinery.layout().fuel_lines[usize::from(line)].blazes[0].centre()
}

/// A client walking into a Lit Blaze is slowed by its own step from the same mask, so it goes just as far as the Host has it go
/// (the Host judging the same inputs), is never put right, and a Shut line slows no one.
#[test]
fn a_clients_slow_in_a_blaze_matches_the_hosts() {
    use crate::net::play::Prediction;
    use crate::players::player::{LocalPlayer, MoveInput};
    // The Bund Line's Blaze is (-42, 38) to (-38, 42): start at its south edge, walking north.
    let start = Vec3::new(-40.0, 0.0, 41.8);
    let walk = |shut: bool| -> (f32, f32, f32) {
        let mut run = refinery(1);
        if shut {
            for on in 0..2 {
                run.world(on).resource_mut::<FuelLines>().show(FuelLineId(1), LinePhase::Shut { elapsed: 0.0 });
            }
        }
        put(&mut run, 1, start);
        run.step(10);
        let from = feet_on(&mut run, 0, 1);
        let mut worst = 0.0f32;
        for _ in 0..20 {
            let world = run.world(1);
            let mut query = world.query_filtered::<&mut MoveInput, With<LocalPlayer>>();
            *query.single_mut(world).unwrap() = MoveInput { axes: Vec2::Y, ..default() };
            run.step(1);
            worst = worst.max(run.world(1).resource::<Prediction>().corrected);
        }
        // Let the Host hear the last of the inputs.
        let world = run.world(1);
        let mut query = world.query_filtered::<&mut MoveInput, With<LocalPlayer>>();
        *query.single_mut(world).unwrap() = MoveInput::default();
        run.step(20);
        let (host, own) = (feet_on(&mut run, 0, 1), feet_on(&mut run, 1, 1));
        ((host - from).with_y(0.0).length(), own.distance(host), worst)
    };
    let (lit, lit_apart, lit_worst) = walk(false);
    let (shut, shut_apart, shut_worst) = walk(true);
    assert!(shut > 1.0, "{shut} m");
    assert!((lit / shut - BLAZE_SPEED_MULT).abs() < 0.06, "the Host has them go {lit} m in the Blaze against {shut} m: ×{}", lit / shut);
    assert!(lit_apart < 0.1 && shut_apart < 0.1, "the client has {lit_apart} m (lit), {shut_apart} m (shut) from the Host's place");
    assert!(lit_worst < 0.1 && shut_worst < 0.1, "the Host put the client right by {lit_worst} (lit), {shut_worst} (shut)");
}

/// A client who goes Down in a Lit Blaze (the Blaze's own ticks take their last Health) is Dragged Clear by the Host with their Last
/// Stand kept, and sees it on its own screen: both games have them outside every Lit Blaze, still Down.
#[test]
fn a_client_down_in_a_blaze_is_dragged_clear_on_its_own_screen() {
    use crate::maps::fire::outside_every_blaze;
    let layout = MapId::Refinery.layout();
    let mut run = Run::on(MapId::Refinery, 1, &|app| {
        super::downs::coop(app);
        crate::players::player::add_teleport_systems(app);
        app.add_plugins(FirePlugin);
    });
    open_every_zone(&mut run);
    run.step(10);
    let spot = middle_of_line(1);
    put(&mut run, 1, spot);
    let player = run.player(0, 1);
    run.world(0).get_mut::<Health>(player).unwrap().current = 10.0;
    run.step(ticks(1.5));
    for on in 0..2 {
        let own = run.player(on, 1);
        assert!(run.world(on).get::<crate::players::last_stand::LastStand>(own).is_some(), "game {on} has them Down, the Last Stand kept");
    }
    let lit = run.world(0).resource::<FuelLines>().burning();
    for on in 0..2 {
        let feet = feet_on(&mut run, on, 1);
        assert!(outside_every_blaze(layout, lit, feet), "game {on} has them at {feet}, in a Blaze");
        assert!(feet.distance(spot) > 1.5, "game {on}: not moved from {spot}");
    }
    let (host, own) = (feet_on(&mut run, 0, 1), feet_on(&mut run, 1, 1));
    assert!(host.distance(own) < 0.5, "the Host has them at {host}, their own screen at {own}");
    // Nothing downs them twice: still one Down, and the Health the Host has is the same on both.
    run.step(ticks(2.0));
    assert_eq!(run.world(1).resource::<super::downs::Downs>().0, 1, "one Down, no loop");
}

/// The Host's Sabotage of a line reaches a client: its Valves are dead on its own screen, a use is refused by the Host, and once a
/// Restore is held they offer again and a use shuts the line on both machines.
#[test]
fn a_client_sees_a_sabotaged_line_refuse_its_valve_and_shut_again_once_restored() {
    use crate::economy::buy::Buyable;
    use crate::enemies::sapper::{Restore, Sabotaged, Target, spawn_restore};
    use crate::maps::fire::{LineNode, Valve};
    let mut run = Run::on(MapId::Refinery, 1, &|app| {
        use crate::enemies::sapper::{prune_restores, restore_bought};
        app.add_plugins(FirePlugin)
            .add_systems(Startup, crate::enemies::sapper::load_assets)
            .add_systems(Update, (restore_bought, prune_restores).chain().in_set(crate::authority::HostSimulation));
    });
    run.give_points(1, 5000);
    run.step(10);
    let offers = |run: &mut Run, on: usize| -> bool {
        let valve = run.find::<Valve>(on, |valve| valve.line == FuelLineId(3));
        run.world(on).get::<Buyable>(valve).is_some_and(|buyable| buyable.price == Some(250))
    };
    assert!(offers(&mut run, 0) && offers(&mut run, 1), "an idle line's Valves offer on both machines");
    // The Sapper's work, as the Host does it: the line is Sabotaged and a Restore waits at each Valve.
    let node = run.find::<LineNode>(0, |node| node.line == FuelLineId(3));
    let points: Vec<Vec3> = {
        let world = run.world(0);
        world.query::<&Valve>().iter(world).filter(|valve| valve.line == FuelLineId(3)).map(|valve| valve.use_point()).collect()
    };
    assert_eq!(points.len(), 2);
    run.world(0).entity_mut(node).insert(Sabotaged);
    run.world(0)
        .run_system_once(move |mut commands: Commands, assets: Res<crate::enemies::sapper::SapperAssets>| {
            for &at in &points {
                spawn_restore(&mut commands, &assets, Target::Entity(node), at);
            }
        })
        .unwrap();
    run.step(20);
    for on in 0..2 {
        assert!(!offers(&mut run, on), "game {on}: a Sabotaged line's Valves are dead");
        assert!(run.world(on).resource::<FuelLines>().sabotaged().contains(FuelLineId(3)), "game {on} knows the line is Sabotaged");
    }
    let client_node = run.find::<LineNode>(1, |node| node.line == FuelLineId(3));
    assert!(run.world(1).get::<Sabotaged>(client_node).is_some(), "the client was told the line is Sabotaged");
    // The client turns a Valve anyway: nothing happens and nothing is charged.
    let valve = run.find::<Valve>(1, |valve| valve.line == FuelLineId(3));
    run.use_buyable(1, valve, false);
    run.step(ticks(1.0));
    for on in 0..2 {
        assert_eq!(phase(&mut run, on, 3), LinePhase::Lit, "game {on}: a Sabotaged line stays Lit");
    }
    assert_eq!(run.balance(1), 5000);
    // The client holds the Restore at a Valve: the Host restores the line, every Restore goes, and the Valves offer again.
    let restore = run.find::<Restore>(1, |_| true);
    let at = run.world(1).get::<Transform>(restore).unwrap().translation;
    put(&mut run, 1, at - Vec3::Y * 1.1);
    run.step(5);
    run.use_buyable(1, restore, true);
    run.step(ticks(4.0));
    for on in 0..2 {
        let world = run.world(on);
        assert!(world.query::<&Restore>().iter(world).next().is_none(), "game {on}: the Restores are gone");
        assert!(offers(&mut run, on), "game {on}: the Valves offer again");
        assert!(!run.world(on).resource::<FuelLines>().sabotaged().contains(FuelLineId(3)));
    }
    assert!(run.balance(1) < 5000, "the Restore was paid for");
    let valve = run.find::<Valve>(1, |valve| valve.line == FuelLineId(3));
    run.use_buyable(1, valve, false);
    run.step(ticks(0.5));
    for on in 0..2 {
        assert!(matches!(phase(&mut run, on, 3), LinePhase::Shut { .. }), "game {on}: a Valve shuts it again: {:?}", phase(&mut run, on, 3));
    }
}
