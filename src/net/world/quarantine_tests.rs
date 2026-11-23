//! The Quarantine over the network: a client sees each stage and the Quarantine Doors swing, a Player
//! who comes back while a Wing is Sealed is told the stage and the Sealed pose, and a client caught in
//! a sweep lands outside on its own screen.

use avian3d::prelude::{Position, SpatialQuery, SpatialQueryFilter};
use bevy::ecs::system::RunSystemOnce;

use super::tests::Run;
use super::*;
use crate::maps::map::MapId;
use crate::maps::shifting::LeafBody;
use crate::players::player::Seat;
use crate::players::respawn::SeatReturned;

fn sanatorium(clients: u8) -> Run {
    Run::on(MapId::Sanatorium, clients, &crate::players::player::add_teleport_systems)
}

/// Every Door open and the Power on, on the Host, which every game then hears.
fn start(run: &mut Run) {
    let host = run.world(0);
    let mut doors = host.query::<&mut Door>();
    for mut door in doors.iter_mut(host) {
        door.open = true;
    }
    run.world(0).resource_mut::<Power>().switch_on();
    run.step(10);
}

fn host_shows(run: &mut Run, stage: QuarantineStage, wing: Option<u8>, elapsed: f32) {
    run.world(0).resource_mut::<Quarantine>().show(stage, wing.map(WingId), elapsed);
}

fn stage(run: &mut Run, on: usize) -> (QuarantineStage, Option<WingId>) {
    let quarantine = run.world(on).resource::<Quarantine>();
    (quarantine.stage(), quarantine.wing())
}

fn phase(run: &mut Run, on: usize, id: u8) -> PiecePhase {
    let world = run.world(on);
    world.query::<&PieceState>().iter(world).find(|state| state.id() == PieceId(id)).unwrap().phase()
}

fn show_piece(run: &mut Run, on: usize, id: u8, phase: PiecePhase) {
    let world = run.world(on);
    world.query::<&mut PieceState>().iter_mut(world).find(|state| state.id() == PieceId(id)).unwrap().show(phase);
}

fn ticks(secs: f32) -> u32 {
    (secs * FIXED_HZ as f32).ceil() as u32
}

/// How far below `from` the first thing in the world is, as game `on` has it.
fn drop_onto(run: &mut Run, on: usize, from: Vec3) -> Option<f32> {
    use crate::layers::GameLayer;
    run.world(on)
        .run_system_once(move |spatial: SpatialQuery| {
            let filter = SpatialQueryFilter::from_mask(GameLayer::World);
            spatial.cast_ray(from, Dir3::NEG_Y, 20.0, true, &filter).map(|hit| hit.distance)
        })
        .unwrap()
}

/// The top of Q1's first Leaf is 0.5 m under this when it is Sealed, and nothing is under this at all
/// but the floor when it is Open.
const OVER_Q1_LEAF: Vec3 = Vec3::new(-40.6, 2.9, 13.77);

#[test]
fn a_client_sees_each_stage_and_the_quarantine_doors_swing() {
    let mut run = sanatorium(1);
    start(&mut run);
    assert_eq!(stage(&mut run, 1), (QuarantineStage::AllClear, None), "the Power starts it, as the client hears");
    host_shows(&mut run, QuarantineStage::Warning, Some(0), 9.9);
    run.step(10);
    assert_eq!(stage(&mut run, 1), (QuarantineStage::Sealing, Some(WingId(0))));
    for id in 0..2 {
        assert!(matches!(phase(&mut run, 1, id), PiecePhase::Swinging { from: 0, to: 1, .. }), "{:?}", phase(&mut run, 1, id));
    }
    assert_eq!(phase(&mut run, 1, 2), PiecePhase::Rest(0), "the other Wings' doors stand Open");
    run.step(ticks(SWING_SECS) + 10);
    assert_eq!(stage(&mut run, 1), (QuarantineStage::Sealed, Some(WingId(0))));
    for on in 0..2 {
        assert_eq!((phase(&mut run, on, 0), phase(&mut run, on, 1)), (PiecePhase::Rest(1), PiecePhase::Rest(1)), "game {on}");
    }
    assert!(drop_onto(&mut run, 1, OVER_Q1_LEAF).is_some_and(|d| (d - 0.5).abs() < 0.05), "the client's Leaf stands across the doorway");
    // Its seconds run on the client to show them, and it stays in the stage.
    run.step(ticks(2.0));
    let elapsed = run.world(1).resource::<Quarantine>().elapsed();
    assert!(elapsed > 1.5 && elapsed < QUARANTINE_SEALED_SECS, "{elapsed}");
    host_shows(&mut run, QuarantineStage::Release, Some(0), QUARANTINE_RELEASE_SECS - 0.1);
    run.step(5);
    assert_eq!(stage(&mut run, 1), (QuarantineStage::Release, Some(WingId(0))));
    run.step(ticks(0.4));
    assert_eq!(stage(&mut run, 1), (QuarantineStage::Opening, Some(WingId(0))));
    assert!(matches!(phase(&mut run, 1, 0), PiecePhase::Swinging { from: 1, to: 0, .. }));
    run.step(ticks(SWING_SECS) + 10);
    assert_eq!(stage(&mut run, 1), (QuarantineStage::AllClear, None));
    assert_eq!((phase(&mut run, 1, 0), phase(&mut run, 1, 1)), (PiecePhase::Rest(0), PiecePhase::Rest(0)));
    assert!(drop_onto(&mut run, 1, OVER_Q1_LEAF).is_none_or(|d| d > 2.5), "Open, the doorway is clear");
}

/// A Player who comes back while a Wing is Sealed is told the stage and the Sealed pose.
#[test]
fn a_late_joiner_arriving_while_a_wing_is_sealed_gets_the_sealed_pose_and_the_stage() {
    let mut run = sanatorium(1);
    start(&mut run);
    host_shows(&mut run, QuarantineStage::Warning, Some(0), 9.9);
    run.step(ticks(SWING_SECS + 1.0));
    assert_eq!(stage(&mut run, 0), (QuarantineStage::Sealed, Some(WingId(0))));
    // The client forgets, as one that was away would have.
    *run.world(1).resource_mut::<Quarantine>() = Quarantine::default();
    for id in 0..2 {
        show_piece(&mut run, 1, id, PiecePhase::Rest(0));
    }
    run.step(5);
    assert!(drop_onto(&mut run, 1, OVER_Q1_LEAF).is_none_or(|d| d > 2.5), "it shows the doors Open");
    run.world(0).write_message(SeatReturned { seat: Seat(1) });
    run.step(10);
    assert_eq!(stage(&mut run, 1), (QuarantineStage::Sealed, Some(WingId(0))));
    assert!(run.world(1).resource::<Quarantine>().elapsed() > 0.5, "with the seconds into it");
    assert_eq!((phase(&mut run, 1, 0), phase(&mut run, 1, 1)), (PiecePhase::Rest(1), PiecePhase::Rest(1)));
    assert!(drop_onto(&mut run, 1, OVER_Q1_LEAF).is_some_and(|d| (d - 0.5).abs() < 0.05), "and the Leaf across the doorway");
    // Every Leaf of it stands at the Sealed pose.
    let rotations = {
        let world = run.world(1);
        let mut query = world.query::<(&LeafBody, &Transform)>();
        query.iter(world).filter(|(body, _)| body.piece.0 < 2).map(|(body, transform)| (body.piece.0, body.index, transform.rotation)).collect::<Vec<_>>()
    };
    let layout = MapId::Sanatorium.layout();
    for (id, index, rotation) in rotations {
        let want = Quat::from_rotation_y(layout.shifting[usize::from(id)].rests[1].yaws[index]);
        assert!(rotation.angle_between(want) < 1e-3, "piece {id} leaf {index}");
    }
}

/// A late joiner who arrives with no Wing out is told the stage too, even All Clear.
#[test]
fn a_late_joiner_is_always_told_the_stage_even_all_clear() {
    let mut run = sanatorium(1);
    start(&mut run);
    *run.world(1).resource_mut::<Quarantine>() = Quarantine::default();
    run.world(0).write_message(SeatReturned { seat: Seat(1) });
    run.step(10);
    assert_eq!(stage(&mut run, 1), (QuarantineStage::AllClear, None));
}

/// A client caught in a sweep lands outside on its own screen.
#[test]
fn a_client_in_a_sweep_lands_outside_on_its_own_screen() {
    let mut run = sanatorium(1);
    start(&mut run);
    let spot = Vec3::new(-40.5, 0.0, 13.2);
    let at = spot + Vec3::Y * crate::players::player::Stance::Standing.half_height();
    for on in 0..2 {
        let player = run.player(on, 1);
        run.world(on).get_mut::<Transform>(player).unwrap().translation = at;
        run.world(on).entity_mut(player).insert(Position(at));
    }
    run.step(10);
    let own = run.player(1, 1);
    let feet = |run: &mut Run| run.world(1).get::<Transform>(own).unwrap().translation - Vec3::Y * crate::players::player::Stance::Standing.half_height();
    assert!(feet(&mut run).distance(spot) < 1.0, "the client stood in the sweep: {}", feet(&mut run));
    host_shows(&mut run, QuarantineStage::Warning, Some(0), 9.9);
    run.step(ticks(1.5));
    let landing = Vec3::new(-40.0, 0.0, 11.5);
    let landed = feet(&mut run);
    assert!(Vec2::new(landed.x - landing.x, landed.z - landing.z).length() < 0.6, "the client's own screen has them at {landed}, the landing {landing}");
    let on_host = run.player(0, 1);
    let host_has = run.world(0).get::<Transform>(on_host).unwrap().translation;
    assert!(Vec2::new(host_has.x - landing.x, host_has.z - landing.z).length() < 0.6, "and so does the Host: {host_has}");
}

/// A client's Crash Bar request moves them on the Host and on their own screen, from a Last Stand too.
#[test]
fn a_clients_crash_bar_request_moves_them_on_the_host_and_on_their_own_screen() {
    use crate::maps::quarantine::CrashBar;
    for last_stand in [false, true] {
        let mut run = sanatorium(1);
        start(&mut run);
        for on in 0..2 {
            for id in 0..2 {
                show_piece(&mut run, on, id, PiecePhase::Rest(1));
            }
        }
        host_shows(&mut run, QuarantineStage::Sealed, Some(0), 1.0);
        let spot = Vec3::new(-40.0, 0.0, 14.6);
        let half = crate::players::player::Stance::Standing.half_height();
        for on in 0..2 {
            let player = run.player(on, 1);
            run.world(on).get_mut::<Transform>(player).unwrap().translation = spot + Vec3::Y * half;
            run.world(on).entity_mut(player).insert(Position(spot + Vec3::Y * half));
        }
        if last_stand {
            for on in 0..2 {
                let player = run.player(on, 1);
                run.world(on).entity_mut(player).insert(crate::players::last_stand::LastStand::shown(10.0, false));
            }
        }
        run.step(ticks(1.0));
        let bar = run.find::<CrashBar>(1, |bar| bar.piece == PieceId(0));
        // The client's own game offers it: a Prompt, in reach.
        assert!(run.world(1).get::<crate::economy::buy::Buyable>(bar).unwrap().range > 0.0, "the Prompt is offered");
        run.use_buyable(1, bar, false);
        run.step(ticks(1.5));
        let landing = Vec3::new(-40.0, 0.0, 11.5);
        let feet = |run: &mut Run, on: usize| {
            let player = run.player(on, 1);
            run.world(on).get::<Transform>(player).unwrap().translation - Vec3::Y * half
        };
        for on in 0..2 {
            let at = feet(&mut run, on);
            assert!(Vec2::new(at.x - landing.x, at.z - landing.z).length() < 0.8, "last_stand {last_stand}, game {on}: {at}");
        }
    }
}
