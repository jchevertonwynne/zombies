//! Out of Bounds, for Players: anyone who gets past the edge of the Map's plan, or falls well
//! below its lowest floor, goes Down at once and is put back where a teammate can reach them. The
//! Host judges it, as it does every Down; a client sees the result in the next snapshot.
//! (Enemies Out of Bounds go the way their stuck rules send them: see `crate::enemies`.)

use bevy::prelude::*;

use crate::authority::HostSimulation;
use crate::enemies::nav::{NavGraph, zone_at};
use crate::enemies::target::half_height_above_feet;
use crate::maps::map::MapId;
use crate::maps::teleporter::in_holding_space;
use crate::players::last_stand::{LastStand, go_down};
use crate::players::player::{Health, Locomotion, Player, Teleport, take_hits};
use crate::players::respawn::AwaitingRespawn;
use crate::state::world_runs;

/// The last spot a Player stood on inside a Zone: where they're put back from if they get Out
/// of Bounds. `None` until they first stand in one, when it's their Map's first Seat.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct Footing(pub Option<Vec3>);

pub struct OutOfBoundsPlugin;

impl Plugin for OutOfBoundsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (note_footing, catch_out_of_bounds)
                .chain()
                .after(take_hits)
                .before(go_down)
                .in_set(HostSimulation)
                .run_if(world_runs),
        );
    }
}

/// Where a Player's feet are.
fn feet_of(transform: &Transform, locomotion: Option<&Locomotion>) -> Vec3 {
    transform.translation - Vec3::Y * half_height_above_feet(locomotion)
}

/// Notes where each Player last stood on the ground inside a Zone.
pub fn note_footing(mut players: Query<(&Transform, Option<&Locomotion>, &mut Footing), With<Player>>) {
    for (transform, locomotion, mut footing) in &mut players {
        let feet = feet_of(transform, locomotion);
        if locomotion.is_some_and(|locomotion| locomotion.grounded) && zone_at(feet + Vec3::Y * 0.1).is_some() {
            footing.0 = Some(feet);
        }
    }
}

/// A Player Out of Bounds goes Down at once, whatever their Health, the same Down as from damage
/// (`go_down` judges it next), and is moved to the in-Zone path node nearest where they last
/// stood. One already in Last Stand is only moved back. A rider waiting in the Teleporter's
/// holding space is not Out of Bounds.
pub fn catch_out_of_bounds(
    mut players: Query<
        (Entity, &Transform, Option<&Locomotion>, &Footing, &mut Health, Has<LastStand>),
        (With<Player>, Without<AwaitingRespawn>),
    >,
    map: Res<MapId>,
    graph: Option<Res<NavGraph>>,
    mut teleport: MessageWriter<Teleport>,
) {
    let layout = map.layout();
    for (player, transform, locomotion, footing, mut health, last_stand) in &mut players {
        let feet = feet_of(transform, locomotion);
        if !layout.out_of_bounds(feet) || in_holding_space(feet) {
            continue;
        }
        if !last_stand {
            health.current = 0.0;
            health.since_hit = 0.0;
        }
        let from = footing.0.unwrap_or(layout.seat_start(0));
        let back = graph.as_ref().filter(|graph| graph.map == *map).map_or(from, |graph| graph.nodes[graph.nearest_in_zone(from)].at);
        teleport.write(Teleport { player, feet: back, facing: None });
    }
}
