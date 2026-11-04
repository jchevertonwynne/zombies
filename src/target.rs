//! Which Player a Zombie goes after, and the Players the Map's hazards can reach.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::last_stand::LastStand;
use crate::map::BarrierId;
use crate::modifier::Modifiers;
use crate::nav::NavGraph;
use crate::player::{LocalPlayer, Player};
use crate::respawn::AwaitingRespawn;
use crate::zone::PlayerZone;
use crate::tuning::*;
use crate::zombie::Zombie;
use crate::zone::ZoneId;

/// Where a Player stands and whether Zombies can go after them.
#[derive(Debug, Clone, Copy)]
pub struct PlayerSpot {
    pub entity: Entity,
    /// The middle of their capsule.
    pub centre: Vec3,
    /// Where their feet stand.
    pub feet: Vec3,
    /// The Zone they stand in.
    pub zone: ZoneId,
    /// Not in a Last Stand and not waiting to Respawn. The one place that decides it is `is_up`.
    pub up: bool,
    /// The Player this machine drives.
    pub local: bool,
}

/// Whether a Player can be gone after: not in a Last Stand and not waiting to Respawn.
/// Disconnected joins the test here.
pub fn is_up(last_stand: bool, awaiting_respawn: bool) -> bool {
    !last_stand && !awaiting_respawn
}

/// The Players on the Map.
#[derive(SystemParam)]
pub struct Players<'w, 's> {
    players: Query<'w, 's, (Entity, &'static Transform, &'static PlayerZone, Has<LastStand>, Has<AwaitingRespawn>, Has<LocalPlayer>), (With<Player>, Without<Zombie>)>,
    modifiers: Query<'w, 's, &'static Modifiers, With<Player>>,
}

impl Players<'_, '_> {
    fn spot((entity, transform, zone, last_stand, awaiting, local): (Entity, &Transform, &PlayerZone, bool, bool, bool)) -> PlayerSpot {
        PlayerSpot {
            entity,
            centre: transform.translation,
            feet: transform.translation - Vec3::Y * (PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0),
            zone: zone.0,
            up: is_up(last_stand, awaiting),
            local,
        }
    }

    /// Every Player, up or not.
    pub fn iter(&self) -> impl Iterator<Item = PlayerSpot> + '_ {
        self.players.iter().map(Self::spot)
    }

    pub fn get(&self, entity: Entity) -> Option<PlayerSpot> {
        self.players.get(entity).ok().map(Self::spot)
    }

    /// The Players something spawns relative to: every one who is up, or everyone if none is.
    pub fn spawn_anchors(&self) -> Vec<PlayerSpot> {
        let up: Vec<PlayerSpot> = self.iter().filter(|spot| spot.up).collect();
        if up.is_empty() { self.iter().collect() } else { up }
    }

    /// How many Players are up.
    pub fn up_count(&self) -> u32 {
        self.iter().filter(|spot| spot.up).count() as u32
    }

    /// One Player, for the rules that don't yet look at them all.
    pub fn first(&self) -> Option<PlayerSpot> {
        self.iter().next()
    }

    /// The Player's stat entries.
    pub fn modifiers(&self, entity: Entity) -> Option<&Modifiers> {
        self.modifiers.get(entity).ok()
    }

    /// The Player closest to `from` in a straight line, one who is up first.
    pub fn nearest(&self, from: Vec3) -> Option<PlayerSpot> {
        self.iter().min_by(|a, b| {
            (!a.up, from.distance(a.feet))
                .partial_cmp(&(!b.up, from.distance(b.feet)))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// The Player a Zombie at `from` is going after: the one it picked (its `Route::target`), or
    /// until it has picked, the nearest.
    pub fn target_of(&self, picked: Option<Entity>, from: Vec3) -> Option<PlayerSpot> {
        picked.and_then(|entity| self.get(entity)).or_else(|| self.nearest(from))
    }
}

/// How far a Zombie at `from` has to walk to reach `to`: the route over the path nodes, plus the
/// walk to the first and from the last, or the straight line when they share a node area.
/// `None` if there's no way through.
pub fn path_distance(
    graph: &NavGraph,
    from: Vec3,
    to: Vec3,
    open_doors: &[(ZoneId, ZoneId)],
    barrier_open: impl Fn(BarrierId) -> bool,
) -> Option<f32> {
    let (start, end) = (graph.nearest(from), graph.nearest(to));
    if start == end {
        return Some(from.distance(to));
    }
    let path = graph.plan(start, end, open_doors, barrier_open)?;
    let nodes: Vec<Vec3> = path.iter().map(|&i| graph.nodes[i].at).collect();
    let walked: f32 = nodes.windows(2).map(|pair| pair[0].distance(pair[1])).sum();
    Some(from.distance(nodes[0]) + walked + nodes[nodes.len() - 1].distance(to))
}

/// The Player who is up and nearest by path to a Zombie at `from` (its feet), or `None` if no
/// Player is up. A Player no path reaches comes after those that one does, nearest in a
/// straight line first.
pub fn nearest_up_player(
    graph: &NavGraph,
    from: Vec3,
    players: impl IntoIterator<Item = PlayerSpot>,
    open_doors: &[(ZoneId, ZoneId)],
    barrier_open: impl Fn(BarrierId) -> bool,
) -> Option<PlayerSpot> {
    let up: Vec<PlayerSpot> = players.into_iter().filter(|player| player.up).collect();
    // One Player up is the pick whatever the walk, so solo plans no route here.
    if let [only] = up[..] {
        return Some(only);
    }
    up.into_iter()
        .map(|player| {
            let walk = path_distance(graph, from, player.feet, open_doors, &barrier_open);
            ((walk.unwrap_or(f32::INFINITY), from.distance(player.feet)), player)
        })
        .min_by(|(a, _), (b, _)| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)))
        .map(|(_, player)| player)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spot(index: u32, feet: Vec3, up: bool) -> PlayerSpot {
        PlayerSpot {
            entity: Entity::from_raw_u32(index).unwrap(),
            centre: feet + Vec3::Y,
            feet,
            zone: ZoneId::StartRoom,
            up,
            local: false,
        }
    }

    #[test]
    fn nearer_by_path_wins_over_nearer_in_a_straight_line() {
        let graph = NavGraph::new();
        let all_open = ZoneId::ALL.iter().flat_map(|&a| ZoneId::ALL.iter().map(move |&b| (a, b))).collect::<Vec<_>>();
        // From the West Hall's middle: the Yard's west end is close through the wall but the way
        // round is up the ramp and over the Balcony, and the Hall's north end is further off but
        // an open walk.
        let zombie = Vec3::new(-21.0, 0.0, 4.0);
        let through_the_wall = Vec3::new(-8.0, 0.0, 0.0);
        let along_the_hall = Vec3::new(-20.0, 0.0, 20.0);
        assert!(zombie.distance(through_the_wall) < zombie.distance(along_the_hall));
        let walk = |to| path_distance(&graph, zombie, to, &all_open, |_| true).unwrap();
        assert!(walk(through_the_wall) > walk(along_the_hall), "the setup needs a long way round");
        let players = [spot(1, through_the_wall, true), spot(2, along_the_hall, true)];
        let picked = nearest_up_player(&graph, zombie, players, &all_open, |_| true).unwrap();
        assert_eq!(picked.entity, players[1].entity);
    }

    #[test]
    fn a_shut_door_makes_the_far_side_the_longer_way() {
        let graph = NavGraph::new();
        let zombie = Vec3::new(0.0, 0.0, 20.0);
        let behind_a_door = Vec3::new(-12.5, 0.0, 20.0);
        let players = [spot(1, behind_a_door, true), spot(2, Vec3::new(12.5, 0.0, 20.0), true)];
        let open = [(ZoneId::StartRoom, ZoneId::WestHall)];
        let picked = nearest_up_player(&graph, zombie, players, &open, |_| false).unwrap();
        assert_eq!(picked.entity, players[0].entity);
    }

    #[test]
    fn players_who_are_not_up_are_never_chosen() {
        let graph = NavGraph::new();
        let zombie = Vec3::new(0.0, 0.0, 0.0);
        let down_beside_it = spot(1, Vec3::new(0.5, 0.0, 0.0), false);
        let up_far_off = spot(2, Vec3::new(11.0, 0.0, -2.0), true);
        let picked = nearest_up_player(&graph, zombie, [down_beside_it, up_far_off], &[], |_| true).unwrap();
        assert_eq!(picked.entity, up_far_off.entity);
        assert!(nearest_up_player(&graph, zombie, [down_beside_it], &[], |_| true).is_none());
        assert!(nearest_up_player(&graph, zombie, [], &[], |_| true).is_none());
    }

    #[test]
    fn the_only_player_up_is_chosen_without_planning_a_path() {
        let graph = NavGraph::new();
        let down = spot(1, Vec3::new(0.5, 0.0, 0.0), false);
        let up = spot(2, Vec3::new(0.0, 0.0, 20.0), true);
        // The barrier test is called when a route from outside the window is planned.
        let planned = std::cell::Cell::new(false);
        let picked = nearest_up_player(&graph, Vec3::new(-7.0, 0.0, 28.0), [down, up], &[], |_| {
            planned.set(true);
            false
        });
        assert_eq!(picked.unwrap().entity, up.entity);
        assert!(!planned.get(), "it planned a path to the only Player up");
    }

    #[test]
    fn the_only_player_up_is_always_chosen_even_with_no_way_through() {
        let graph = NavGraph::new();
        // Outside a shut window: no route in.
        let outside = Vec3::new(-7.0, 0.0, 28.0);
        let inside = spot(1, Vec3::new(0.0, 0.0, 20.0), true);
        let picked = nearest_up_player(&graph, outside, [inside], &[], |_| false);
        assert_eq!(picked.unwrap().entity, inside.entity);
    }

    #[test]
    fn a_player_no_path_reaches_comes_after_one_that_one_does() {
        let graph = NavGraph::new();
        let outside = Vec3::new(-7.0, 0.0, 28.0);
        let near_but_cut_off = spot(1, Vec3::new(0.0, 0.0, 20.0), true);
        let reachable = spot(2, Vec3::new(-7.0, 0.0, 33.0), true);
        let picked = nearest_up_player(&graph, outside, [near_but_cut_off, reachable], &[], |_| false);
        assert_eq!(picked.unwrap().entity, reachable.entity);
    }
}
