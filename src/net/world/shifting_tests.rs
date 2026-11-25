//! The Swing Bridges over the network: a client sees a Swing as the Host runs it, its Leaves
//! stand at the new Rest, and a Player who comes back is told every Piece's phase.

use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::ecs::system::RunSystemOnce;

use super::tests::Run;
use super::*;
use crate::maps::map::{MapId};
use crate::players::player::Seat;
use crate::players::respawn::SeatReturned;

fn polder(clients: u8) -> Run {
    Run::on(MapId::Polder, clients, &|_| {})
}

fn phase(run: &mut Run, on: usize, id: u8) -> PiecePhase {
    let world = run.world(on);
    world.query::<&PieceState>().iter(world).find(|state| state.id() == PieceId(id)).unwrap().phase()
}

fn show(run: &mut Run, on: usize, id: u8, phase: PiecePhase) {
    let world = run.world(on);
    world.query::<&mut PieceState>().iter_mut(world).find(|state| state.id() == PieceId(id)).unwrap().show(phase);
}

/// How far down from `from` a ray hits something solid, as game `on` has the world.
fn drop_onto(run: &mut Run, on: usize, from: Vec3) -> Option<f32> {
    use crate::layers::GameLayer;
    run.world(on)
        .run_system_once(move |spatial: SpatialQuery| {
            let filter = SpatialQueryFilter::from_mask(GameLayer::World);
            spatial.cast_ray(from, Dir3::NEG_Y, 20.0, true, &filter).map(|hit| hit.distance)
        })
        .unwrap()
}

#[test]
fn a_client_sees_warning_then_swinging_then_rest_with_its_deck_at_the_new_rest() {
    let mut run = polder(1);
    run.give_points(1, 5000);
    run.step(10);
    let lever = run.find::<Lever>(1, |lever| lever.piece == PieceId(0));
    // Over the Along deck's middle, and the Span deck's.
    let along = Vec3::new(-14.0, 3.0, -10.0);
    let span = Vec3::new(-22.0, 3.0, -6.0);
    assert!(drop_onto(&mut run, 1, span).is_some_and(|d| (d - 3.0).abs() < 0.05), "the deck stands at Span");
    assert!(drop_onto(&mut run, 1, along).is_none_or(|d| d > 3.5), "and nothing is over the water at Along");
    run.use_buyable(1, lever, false);
    let mut seen: Vec<PiecePhase> = Vec::new();
    for _ in 0..600 {
        run.step(1);
        let now = phase(&mut run, 1, 0);
        if seen.last().is_none_or(|last| std::mem::discriminant(last) != std::mem::discriminant(&now)) {
            seen.push(now);
        }
        if matches!(now, PiecePhase::Cooling { .. }) {
            break;
        }
    }
    let kinds: Vec<&str> = seen
        .iter()
        .map(|phase| match phase {
            PiecePhase::Rest(_) => "rest",
            PiecePhase::Warning { .. } => "warning",
            PiecePhase::Waiting { .. } => "waiting",
            PiecePhase::Swinging { .. } => "swinging",
            PiecePhase::Cooling { .. } => "cooling",
        })
        .collect();
    assert_eq!(kinds, ["rest", "warning", "swinging", "cooling"], "{seen:?}");
    assert_eq!(run.balance(1), 4500, "the Lever cost 500");
    run.step(5);
    assert!(drop_onto(&mut run, 1, along).is_some_and(|d| (d - 3.0).abs() < 0.05), "the deck now lies along the bank");
    assert!(drop_onto(&mut run, 1, span).is_none_or(|d| d > 3.5), "and has left Span");
    assert!(matches!(phase(&mut run, 0, 0), PiecePhase::Cooling { at: 1, .. }), "the Host agrees");
}

#[test]
fn a_player_who_comes_back_is_told_every_pieces_phase() {
    let mut run = polder(1);
    run.step(5);
    let moved = [
        PiecePhase::Cooling { at: 1, elapsed: 4.0 },
        PiecePhase::Rest(0),
        PiecePhase::Swinging { from: 1, to: 0, elapsed: 2.0 },
        PiecePhase::Rest(1),
    ];
    for (id, phase) in moved.into_iter().enumerate() {
        show(&mut run, 0, id as u8, phase);
    }
    run.step(5);
    // The client forgets, as one that was away would have.
    for id in 0..4u8 {
        show(&mut run, 1, id, PiecePhase::Rest(0));
    }
    run.world(0).write_message(SeatReturned { seat: Seat(1) });
    run.step(3);
    for (id, want) in moved.into_iter().enumerate() {
        let got = phase(&mut run, 1, id as u8);
        assert_eq!(std::mem::discriminant(&got), std::mem::discriminant(&want), "piece {id}: {got:?} vs {want:?}");
    }
    assert!(matches!(phase(&mut run, 1, 0), PiecePhase::Cooling { at: 1, .. }));
    assert!(matches!(phase(&mut run, 1, 2), PiecePhase::Swinging { from: 1, to: 0, .. }));
}

fn polder_with_tide(clients: u8) -> Run {
    Run::on(MapId::Polder, clients, &|app| {
        app.init_resource::<crate::maps::tide::Tide>();
    })
}

fn set_tide(run: &mut Run, on: usize, phase: crate::maps::tide::TidePhase, turn: crate::maps::tide::TurnState) {
    *run.world(on).resource_mut::<crate::maps::tide::Tide>() = crate::maps::tide::Tide { phase, turn };
}

/// The Lock Gates swing on the Host's Tide and a client sees them swing and come to Rest, its
/// walkway present only over the gate that is Shut.
#[test]
fn a_client_sees_the_lock_gates_swing_with_the_tide() {
    use crate::maps::tide::{TidePhase, TurnState};
    let mut run = polder_with_tide(1);
    run.step(10);
    let inland = Vec3::new(-11.0, 3.0, 2.5);
    let sea = Vec3::new(11.0, 3.0, 21.5);
    let walkway = |run: &mut Run, over: Vec3| drop_onto(run, 1, over).is_some_and(|d| (d - 3.0).abs() < 0.05);
    assert!(walkway(&mut run, inland) && !walkway(&mut run, sea), "at Low the Inland Gate is Shut");
    set_tide(&mut run, 0, TidePhase::Rising, TurnState::Turning { elapsed: 0.0 });
    run.step(5);
    assert!(matches!(phase(&mut run, 1, 4), PiecePhase::Swinging { from: 0, to: 1, .. }), "{:?}", phase(&mut run, 1, 4));
    assert!(matches!(phase(&mut run, 1, 5), PiecePhase::Swinging { from: 1, to: 0, .. }), "{:?}", phase(&mut run, 1, 5));
    // The Host's Tide lands (the plugin that does it isn't in these tests).
    run.step(300);
    set_tide(&mut run, 0, TidePhase::High, TurnState::Still);
    run.step(10);
    assert_eq!((phase(&mut run, 0, 4), phase(&mut run, 1, 4)), (PiecePhase::Rest(1), PiecePhase::Rest(1)));
    assert_eq!((phase(&mut run, 0, 5), phase(&mut run, 1, 5)), (PiecePhase::Rest(0), PiecePhase::Rest(0)));
    assert!(!walkway(&mut run, inland) && walkway(&mut run, sea), "at High the Sea Gate is Shut");
}

/// A Player who comes back mid-Run is told both gates' Rests.
#[test]
fn a_late_joiner_gets_both_lock_gates_rests() {
    use crate::maps::tide::{TidePhase, TurnState};
    let mut run = polder_with_tide(1);
    run.step(5);
    set_tide(&mut run, 0, TidePhase::High, TurnState::Still);
    run.step(10);
    assert_eq!((phase(&mut run, 0, 4), phase(&mut run, 0, 5)), (PiecePhase::Rest(1), PiecePhase::Rest(0)));
    // The client forgets, as one that was away would have.
    show(&mut run, 1, 4, PiecePhase::Rest(0));
    show(&mut run, 1, 5, PiecePhase::Rest(1));
    run.world(0).write_message(SeatReturned { seat: Seat(1) });
    run.step(3);
    assert_eq!((phase(&mut run, 1, 4), phase(&mut run, 1, 5)), (PiecePhase::Rest(1), PiecePhase::Rest(0)));
}

// ---- The Archive's Ranges ----

fn archive(clients: u8) -> Run {
    Run::on(MapId::Archive, clients, &|_| {})
}

fn ticks(secs: f32) -> u32 {
    (secs * FIXED_HZ as f32).ceil() as u32
}

/// Whether, as game `on` has it, the first thing under (x, 11.5, 11) is a Bookcase's top (0.5 m down) and not the floor (3.5 m down).
fn bookcase_at(run: &mut Run, on: usize, x: f32) -> bool {
    drop_onto(run, on, Vec3::new(x, 11.5, 11.0)).is_some_and(|d| (d - 0.5).abs() < 0.05)
}

/// A client's Wheel request rolls the Great Range on the Host and on its own screen, through warning, roll and cooldown, and the Bookcases
/// stand at the new Rest on both.
#[test]
fn a_clients_wheel_request_rolls_the_range_on_both_machines() {
    let mut run = archive(1);
    run.step(10);
    // The Aisle is in the middle at the start: its Bookcases are at the ends, x -0.3 is open floor, and x -3.3 and 3.3 are shelves.
    for on in 0..2 {
        assert_eq!(phase(&mut run, on, 0), PiecePhase::Rest(2));
        assert!(!bookcase_at(&mut run, on, -0.3), "game {on}: the Aisle is open");
        assert!(bookcase_at(&mut run, on, -3.3) && bookcase_at(&mut run, on, 3.3), "game {on}");
    }
    let wheel = run.find::<WheelPost>(1, |wheel| wheel.piece == PieceId(0) && wheel.rest == 0);
    assert_eq!(run.world(1).get::<Buyable>(wheel).map(|buyable| (buyable.prompt.as_str(), buyable.price)), Some(("Press F to open this aisle", None)), "the client's own Wheel offers it");
    let balance = run.balance(1);
    run.use_buyable(1, wheel, false);
    run.step(ticks(0.5));
    for on in 0..2 {
        assert!(matches!(phase(&mut run, on, 0), PiecePhase::Warning { from: 2, to: 0, .. }), "game {on}: {:?}", phase(&mut run, on, 0));
    }
    let mut seen: Vec<&str> = Vec::new();
    for _ in 0..ticks(SWING_WARN_SECS + SWING_SECS + 1.0) {
        run.step(1);
        let now = match phase(&mut run, 1, 0) {
            PiecePhase::Rest(_) => "rest",
            PiecePhase::Warning { .. } => "warning",
            PiecePhase::Waiting { .. } => "waiting",
            PiecePhase::Swinging { .. } => "swinging",
            PiecePhase::Cooling { .. } => "cooling",
        };
        if seen.last() != Some(&now) {
            seen.push(now);
        }
    }
    assert_eq!(seen, ["warning", "swinging", "cooling"], "the client saw every phase");
    for on in 0..2 {
        assert!(matches!(phase(&mut run, on, 0), PiecePhase::Cooling { at: 0, .. }), "game {on}");
        // Rest 0: the Aisle is at the west end, x -3.4 is open floor and x -0.3 and 3.3 are shelves.
        assert!(!bookcase_at(&mut run, on, -3.4), "game {on}: the Aisle moved west");
        assert!(bookcase_at(&mut run, on, -0.3) && bookcase_at(&mut run, on, 3.3), "game {on}");
    }
    assert_eq!(run.balance(1), balance, "free");
    // Eight seconds on, it is back at Rest on both.
    run.step(ticks(RANGE_COOLDOWN_SECS + 0.5));
    for on in 0..2 {
        assert_eq!(phase(&mut run, on, 0), PiecePhase::Rest(0), "game {on}");
    }
}

/// A Player who comes back is told the Range's Rest, and the Bookcases stand there on their machine.
#[test]
fn a_late_joiner_gets_the_current_rest_of_every_range() {
    let mut run = archive(1);
    run.step(5);
    let moved = [PiecePhase::Rest(0), PiecePhase::Cooling { at: 2, elapsed: 3.0 }, PiecePhase::Rest(1), PiecePhase::Swinging { from: 3, to: 1, elapsed: 2.0 }, PiecePhase::Rest(0)];
    for (id, phase) in moved.into_iter().enumerate() {
        show(&mut run, 0, id as u8, phase);
    }
    run.step(5);
    // The client forgets, as one that was away would have.
    for id in 0..5u8 {
        show(&mut run, 1, id, PiecePhase::Rest(RANGE_START[usize::from(id)]));
    }
    run.step(3);
    run.world(0).write_message(SeatReturned { seat: Seat(1) });
    run.step(5);
    for (id, want) in moved.into_iter().enumerate() {
        let got = phase(&mut run, 1, id as u8);
        assert_eq!(std::mem::discriminant(&got), std::mem::discriminant(&want), "Range {id}: {got:?} vs {want:?}");
        assert_eq!(got.standing(), want.standing(), "Range {id}");
    }
    assert!(matches!(phase(&mut run, 1, 3), PiecePhase::Swinging { from: 3, to: 1, .. }));
    // The Great Range is at Rest 0 on the client: the Aisle at the west end.
    run.step(ticks(1.0));
    assert!(!bookcase_at(&mut run, 1, -3.4) && bookcase_at(&mut run, 1, -0.3), "the late joiner's Bookcases stand at Rest 0");
}

/// A client who walks into a Bookcase mid-roll is stopped by its collider, on its own machine and on the Host's.
#[test]
fn a_client_walking_into_a_closing_bookcase_is_stopped_by_its_collider() {
    use crate::players::player::{LocalPlayer, MoveInput, Stance};
    let mut run = archive(1);
    run.step(10);
    // Halfway through the roll from Rest 2 to Rest 3 (the Aisle closing from the east), the third Bookcase is across x 0 to 1.2.
    let mid = PiecePhase::Swinging { from: 2, to: 3, elapsed: SWING_SECS / 2.0 };
    for on in 0..2 {
        show(&mut run, on, 0, mid);
    }
    let half = Stance::Standing.half_height();
    // In the gate's corridor (z 14.5 to 15.7, between the Bookcases' face and the grille), where the Bookcase's face is the only thing ahead.
    let start = Vec3::new(0.6, 8.0 + half, 15.2);
    for on in 0..2 {
        let player = run.player(on, 1);
        run.world(on).get_mut::<Transform>(player).unwrap().translation = start;
        run.world(on).entity_mut(player).insert(avian3d::prelude::Position(start));
    }
    let feet_z = |run: &mut Run, on: usize| {
        let player = run.player(on, 1);
        run.world(on).get::<Transform>(player).unwrap().translation.z
    };
    for _ in 0..40 {
        let world = run.world(1);
        let mut query = world.query_filtered::<&mut MoveInput, With<LocalPlayer>>();
        *query.single_mut(world).unwrap() = MoveInput { axes: Vec2::Y, ..default() };
        run.step(1);
    }
    // Their capsule's centre stays out of the Bookcase's face (z 14.5) by at least its radius, on both machines.
    let (own, host) = (feet_z(&mut run, 1), feet_z(&mut run, 0));
    assert!(own < start.z - 0.1, "the client did walk: {own}");
    assert!(own > 14.5 + 0.2, "the client's own screen stopped them at the Bookcase: {own}");
    assert!(host > 14.5 + 0.2, "and so did the Host's: {host}");
}

/// Where each Range stands at the start of a Run, in order.
const RANGE_START: [u8; 5] = [2, 0, 0, 3, 2];
