//! A Player Out of Bounds goes Down and is put back where a teammate can reach them.

use super::*;
use crate::economy::perk::{Perk, Perks};
use crate::enemies::nav::NavGraph;
use crate::enemies::target::half_height_above_feet;
use crate::players::last_stand::{Down, LastStand, LastStandPlugin};
use crate::players::out_of_bounds::{Footing, OutOfBoundsPlugin};
use crate::state::{GameState, RunType};
use crate::run::stats::StatsPlugin;

/// The Downs heard so far.
#[derive(Resource, Default)]
struct Downs(u32);

fn count_downs(mut downs: ResMut<Downs>, mut heard: MessageReader<Down>) {
    downs.0 += heard.read().count() as u32;
}

/// One Player in a solo Run on the Greybox, holding `perks`, with the rules of Downs and Out of Bounds.
fn solo(perks: &[Perk]) -> (Sim, Entity) {
    let (mut sim, local, teammate) = two_players_with(|app| {
        app.insert_resource(RunType::Solo)
            .add_plugins((LastStandPlugin, StatsPlugin, OutOfBoundsPlugin))
            .add_message::<crate::players::revive::Revived>()
            .init_resource::<Downs>()
            .add_systems(Update, (take_hits, count_downs.after(crate::players::last_stand::go_down)));
    });
    let world = sim.world_mut();
    world.entity_mut(teammate).despawn();
    world.entity_mut(local).insert(Perks(perks.to_vec()));
    sim.run(10);
    (sim, local)
}

fn downs(sim: &mut Sim) -> u32 {
    sim.world_mut().resource::<Downs>().0
}

/// Where `player`'s feet are, whatever shape their body is in.
fn feet_of(sim: &mut Sim, player: Entity) -> Vec3 {
    let world = sim.world_mut();
    world.get::<Transform>(player).unwrap().translation - Vec3::Y * half_height_above_feet(world.get::<Locomotion>(player))
}

/// The path node a Player who last stood at `footing` is put back on.
fn put_back_at(sim: &mut Sim, footing: Vec3) -> Vec3 {
    let graph = sim.world_mut().resource::<NavGraph>();
    graph.nodes[graph.nearest_in_zone(footing)].at
}

/// Somewhere under the Greybox, more than `OUT_OF_BOUNDS_DROP` below its lowest floor.
fn under_the_map() -> Vec3 {
    Vec3::new(0.0, MapId::Greybox.layout().lowest_floor() - OUT_OF_BOUNDS_DROP - 0.5, 20.0)
}

/// Just past the west edge of the Greybox's plan.
fn past_the_plan() -> Vec3 {
    let (min, _) = MapId::Greybox.layout().plan;
    Vec3::new(min.x - 1.0, 0.0, 20.0)
}

#[test]
fn solo_without_quick_revive_a_player_out_of_bounds_ends_the_run() {
    for out in [past_the_plan(), under_the_map()] {
        let (mut sim, local) = solo(&[]);
        sim.send(Teleport { player: local, feet: out, facing: None });
        // Frame by frame: the harness can't run on past a Game Over (its Pause state goes with
        // Playing), so the test reads the Game Over still pending in the frame of the Down.
        for _ in 0..3 {
            if downs(&mut sim) == 0 {
                sim.run(1);
            }
        }
        assert_eq!(downs(&mut sim), 1, "no Down at {out}");
        let over = matches!(sim.world_mut().resource::<NextState<GameState>>(), NextState::Pending(GameState::GameOver));
        assert!(over, "the Run goes on after {out}");
    }
}

#[test]
fn a_player_out_of_bounds_goes_down_and_is_put_back_on_the_path_node_nearest_where_they_stood() {
    for out in [past_the_plan(), under_the_map()] {
        let (mut sim, local) = solo(&[Perk::QuickRevive, Perk::Juggernog]);
        // They walk a little way into the Start Room and stand there.
        let stood = Vec3::new(-6.0, 0.0, 18.0);
        place(&mut sim, local, stood);
        sim.run(10);
        let footing = sim.world_mut().get::<Footing>(local).unwrap().0.expect("they've stood in a Zone");
        assert!(footing.distance(stood) < 0.2, "the last footing is {footing}");
        sim.send(Teleport { player: local, feet: out, facing: None });
        sim.run(3);
        assert_eq!(downs(&mut sim), 1, "no Down at {out}, with Juggernog");
        assert!(sim.world_mut().get::<LastStand>(local).is_some(), "Quick Revive gives a Last Stand");
        assert!(sim.world_mut().get::<Perks>(local).unwrap().0.is_empty(), "the Down takes the Perks");
        sim.run(10);
        let node = put_back_at(&mut sim, footing);
        let back = feet_of(&mut sim, local);
        assert!(back.distance(node) < 0.3, "put back at {back}, not the node at {node}");
        assert!(!MapId::Greybox.layout().out_of_bounds(back));
        assert_eq!(downs(&mut sim), 1, "a second Down");
    }
}

#[test]
fn a_player_in_last_stand_out_of_bounds_is_only_put_back() {
    let (mut sim, local) = solo(&[Perk::QuickRevive]);
    sim.world_mut().get_mut::<Health>(local).unwrap().current = 0.0;
    sim.run(3);
    assert!(sim.world_mut().get::<LastStand>(local).is_some());
    assert_eq!(downs(&mut sim), 1);
    let footing = sim.world_mut().get::<Footing>(local).unwrap().0.unwrap();
    sim.send(Teleport { player: local, feet: past_the_plan(), facing: None });
    sim.run(15);
    assert_eq!(downs(&mut sim), 1, "a second Down from Last Stand");
    assert!(sim.world_mut().get::<LastStand>(local).is_some());
    let node = put_back_at(&mut sim, footing);
    let back = feet_of(&mut sim, local);
    assert!(back.distance(node) < 0.3, "put back at {back}, not the node at {node}");
    assert_eq!(sim.world_mut().get::<Locomotion>(local).unwrap().shape(), Stance::Prone, "still in the Down pose");
}

/// A rider waiting in the Teleporter's holding space, past the Greybox's plan, isn't Out of Bounds.
#[test]
fn the_teleporters_holding_space_is_not_out_of_bounds() {
    let (mut sim, local) = solo(&[]);
    let holding = crate::maps::teleporter::holding_feet(0);
    assert!(MapId::Greybox.layout().out_of_bounds(holding), "the holding space is inside the plan now");
    // Its floor, which the Teleporter builds.
    sim.world_mut().spawn((
        RigidBody::Static,
        Collider::cuboid(12.0, 0.2, 6.0),
        Transform::from_translation(holding - Vec3::Y * 0.1),
        CollisionLayers::new(crate::layers::GameLayer::World, LayerMask::ALL),
    ));
    sim.send(Teleport { player: local, feet: holding, facing: None });
    sim.run(30);
    assert_eq!(downs(&mut sim), 0);
}
