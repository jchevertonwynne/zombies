//! Out of Bounds, for Players: anyone who gets past the edge of the Map's plan, or falls well
//! below its lowest floor, goes Down at once and is put back where a teammate can reach them. The
//! Host judges it, as it does every Down; a client sees the result in the next snapshot.
//! (Enemies Out of Bounds go the way their stuck rules send them: see `crate::enemies`.)

use bevy::prelude::*;

use crate::authority::HostSimulation;
use crate::enemies::nav::{NavGraph, zone_at};
use crate::enemies::target::half_height_above_feet;
use crate::economy::power_up::Dropped;
use crate::maps::map::MapId;
use crate::maps::tide::{Tide, TideLanded};
use crate::maps::water::{TideLevel, Wading, swamped, water_at};
use crate::maps::zone::{ActiveZones, ZoneId};
use crate::maps::teleporter::in_holding_space;
use crate::players::last_stand::{LastStand, go_down};
use crate::players::player::{Health, Invincible, Locomotion, Player, Stance, Teleport, take_hits};
use crate::players::respawn::AwaitingRespawn;
use crate::state::world_runs;

/// The last spot a Player stood on inside a Zone: where they're put back from if they get Out
/// of Bounds. `None` until they first stand in one, when it's their Map's first Seat.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct Footing(pub Option<Vec3>);

pub struct OutOfBoundsPlugin;

impl Plugin for OutOfBoundsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<TideLanded>().add_message::<Teleport>().add_systems(
            Update,
            (note_footing, washed_up.after(crate::maps::tide::TideTurn), wade_into_deep, catch_out_of_bounds)
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
pub fn note_footing(mut players: Query<(&Transform, Option<&Locomotion>, &mut Footing), With<Player>>, map: Res<MapId>) {
    for (transform, locomotion, mut footing) in &mut players {
        let feet = feet_of(transform, locomotion);
        if locomotion.is_some_and(|locomotion| locomotion.grounded) && zone_at(*map, feet + Vec3::Y * 0.1).is_some() {
            footing.0 = Some(feet);
        }
    }
}

/// A Player Out of Bounds goes Down at once, whatever their Health, the same Down as from damage
/// (`go_down` judges it next), and is moved to the in-Zone path node nearest where they last
/// stood. One already in Last Stand, or Invincible, is only moved back. A rider waiting in the Teleporter's
/// holding space is not Out of Bounds. A flying Player (noclip) is never Down for it, and one who
/// comes down from a flight Out of Bounds is only put back, with no Down.
pub fn catch_out_of_bounds(
    mut players: Query<
        (Entity, &Transform, Option<&mut Locomotion>, &Footing, &mut Health, Has<LastStand>, Has<Invincible>),
        (With<Player>, Without<AwaitingRespawn>),
    >,
    map: Res<MapId>,
    graph: Option<Res<NavGraph>>,
    mut teleport: MessageWriter<Teleport>,
) {
    let layout = map.layout();
    for (player, transform, mut locomotion, footing, mut health, last_stand, invincible) in &mut players {
        if locomotion.as_deref().is_some_and(Locomotion::flying) {
            continue;
        }
        let landed = locomotion.as_deref_mut().is_some_and(|locomotion| std::mem::take(&mut locomotion.left_flight));
        let feet = feet_of(transform, locomotion.as_deref());
        // Deep Water is not Out of Bounds for a Player: `wade_into_deep` washes them up from it.
        if !layout.off_the_map(feet) || in_holding_space(feet) {
            continue;
        }
        if !last_stand && !invincible && !landed {
            health.current = 0.0;
            health.since_hit = 0.0;
        }
        let from = footing.0.unwrap_or(layout.seat_start(0));
        let back = graph.as_ref().filter(|graph| graph.map == *map).map_or(from, |graph| put_back_node(graph, layout, from));
        teleport.write(Teleport { player, feet: back, facing: None });
    }
}

/// Where a Player Out of Bounds is put back, and so where a Last Stand and its Quick Revive stand
/// them up: the in-Zone path node nearest `from` that is never Deep Water, at any Tide level, so
/// no Tide leaves them wading in the next Turn.
pub fn put_back_node(graph: &NavGraph, layout: &crate::maps::map::Layout, from: Vec3) -> Vec3 {
    let dry = |node: &crate::enemies::nav::Node| TideLevel::ALL.iter().all(|&level| water_at(layout, level, node.at) != Some(Wading::Deep));
    graph.nodes[graph.nearest_in_zone_where(from, dry)].at
}

/// A Player whose feet are in Deep Water, however they got there (the Tide landing is
/// `washed_up`'s), is Washed Up to the nearest dry or Shallow path node: no Down, no damage, their
/// Last Stand kept, their speed gone. Deep Water is a wall to Players, so it takes a shove, a
/// ride or a missing wall to get in. The Host judges it; a client sees the move in the snapshot.
pub fn wade_into_deep(
    mut players: Query<(Entity, &mut Transform, Option<&Locomotion>), (With<Player>, Without<AwaitingRespawn>)>,
    map: Res<MapId>,
    tide: Option<Res<Tide>>,
    graph: Option<Res<NavGraph>>,
    open: Option<Res<ActiveZones>>,
    mut teleport: MessageWriter<Teleport>,
) {
    let layout = map.layout();
    if layout.water.is_empty() {
        return;
    }
    let level = tide.map_or_else(TideLevel::default, |tide| tide.level());
    let Some(graph) = graph.filter(|graph| graph.map == *map) else { return };
    for (player, mut transform, locomotion) in &mut players {
        if locomotion.is_some_and(Locomotion::flying) {
            continue;
        }
        let feet = feet_of(&transform, locomotion);
        if water_at(layout, level, feet) != Some(Wading::Deep) || in_holding_space(feet) {
            continue;
        }
        let Some(to) = washed_up_target(&graph, layout, level, feet, zone_at(*map, feet + Vec3::Y * 0.1), open.as_deref()) else { continue };
        transform.translation = to + Vec3::Y * (Stance::Standing.half_height() + 0.05);
        teleport.write(Teleport { player, feet: to, facing: None });
    }
}

/// The path node a Washed Up body at `feet` goes to with the Tide at `tide`: the nearest in an open
/// Zone that is dry or Shallow, their own Zone (`own`) winning ties. `None` if there is no such.
pub fn washed_up_target(
    graph: &NavGraph,
    layout: &crate::maps::map::Layout,
    tide: TideLevel,
    feet: Vec3,
    own: Option<ZoneId>,
    open: Option<&ActiveZones>,
) -> Option<Vec3> {
    graph
        .nodes
        .iter()
        .filter(|node| !node.vent)
        .filter(|node| node.zone.is_some_and(|zone| open.is_none_or(|open| open.0.contains(zone))))
        .filter(|node| water_at(layout, tide, node.at) != Some(Wading::Deep))
        .map(|node| {
            // Ties are within a few centimetres of one another.
            let distance = (node.at.distance(feet) * 20.0).round();
            ((distance, node.zone != own), node.at)
        })
        .min_by(|a, b| a.0.0.total_cmp(&b.0.0).then(a.0.1.cmp(&b.0.1)))
        .map(|(_, at)| at)
}

/// When the Tide lands, a Player whose feet are in water that is now Deep (or in the air over it)
/// is Washed Up: moved to the nearest dry or Shallow path node in an open Zone, with no damage and
/// no Down, their facing kept and their speed gone. A Player already Down keeps their Last Stand.
/// So are Power-ups lying in it. It runs before `catch_out_of_bounds`, so the Tide never sends
/// anyone Out of Bounds: the body is moved at once, and the `Teleport` carries the physics.
pub fn washed_up(
    mut landed: MessageReader<TideLanded>,
    mut players: Query<(Entity, &mut Transform, Option<&Locomotion>), (With<Player>, Without<AwaitingRespawn>)>,
    mut drops: Query<&mut Dropped>,
    map: Res<MapId>,
    graph: Option<Res<NavGraph>>,
    open: Option<Res<ActiveZones>>,
    mut teleport: MessageWriter<Teleport>,
) {
    let Some(TideLanded { level }) = landed.read().last().copied() else { return };
    let layout = map.layout();
    let Some(graph) = graph.filter(|graph| graph.map == *map) else { return };
    for (player, mut transform, locomotion) in &mut players {
        // A flying Player (noclip) is nobody's to wash up.
        if locomotion.is_some_and(Locomotion::flying) {
            continue;
        }
        let feet = feet_of(&transform, locomotion);
        let airborne = locomotion.is_some_and(|locomotion| !locomotion.grounded);
        let caught = if airborne { swamped(layout, level, feet) } else { water_at(layout, level, feet) == Some(Wading::Deep) };
        if !caught {
            continue;
        }
        let Some(to) = washed_up_target(&graph, layout, level, feet, zone_at(*map, feet + Vec3::Y * 0.1), open.as_deref()) else { continue };
        transform.translation = to + Vec3::Y * (Stance::Standing.half_height() + 0.05);
        teleport.write(Teleport { player, feet: to, facing: None });
    }
    for mut dropped in &mut drops {
        let feet = dropped.at() - Vec3::Y * crate::tuning::POWER_UP_FLOAT_HEIGHT;
        if swamped(layout, level, feet)
            && let Some(to) = washed_up_target(&graph, layout, level, feet, zone_at(*map, feet + Vec3::Y * 0.1), open.as_deref())
        {
            dropped.wash_to(to + Vec3::Y * crate::tuning::POWER_UP_FLOAT_HEIGHT);
        }
    }
}
