//! The Swing Bridges over the network: a client sees a Swing as the Host runs it, its Leaves
//! stand at the new Rest, and a Player who comes back is told every Piece's phase.

use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::ecs::system::RunSystemOnce;

use super::tests::Run;
use super::*;
use crate::maps::map::{MapId, POLDER_X};
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
    let along = Vec3::new(POLDER_X - 14.0, 3.0, -10.0);
    let span = Vec3::new(POLDER_X - 22.0, 3.0, -6.0);
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
    let inland = Vec3::new(POLDER_X - 11.0, 3.0, 2.5);
    let sea = Vec3::new(POLDER_X + 11.0, 3.0, 21.5);
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
