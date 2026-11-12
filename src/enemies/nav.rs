//! Zombie navigation: a hand-authored graph of path nodes, planned over with A* (ADR 0001).

use avian3d::prelude::*;
use bevy::prelude::*;
use pathfinding::prelude::astar;

use crate::maps::barrier::Barriers;
use crate::debug::DebugView;
use crate::maps::door::Door;
use crate::maps::map::{BarrierId, MapId};
use crate::state::GameState;
use crate::tuning::*;
use crate::maps::zone::ZoneId;

/// How many of the nodes nearest a body `NavGraph::near_where` tries.
const NEAREST_TRIES: usize = 6;

/// What gates an edge of the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// Always open: across a room, or up and down the steps and ramps.
    Open,
    /// Through the Door between two Zones: open once it's bought.
    Door(ZoneId, ZoneId),
    /// In through a window, from outside only: open when no Planks are left.
    Barrier(BarrierId),
    /// From the first node to the second only: the drop off the Balcony and the Vents.
    OneWay,
}

impl Gate {
    /// Whether the edge can only be walked from its first node to its second.
    fn one_way(self) -> bool {
        matches!(self, Gate::Barrier(_) | Gate::OneWay)
    }
}

/// Whether a Zombie can walk an edge with this gate right now. `open_doors` lists the joins of
/// the bought Doors; `barrier_open` says whether a Barrier has lost all its Planks (see
/// `Barriers::is_open`).
pub fn passable(gate: Gate, open_doors: &[(ZoneId, ZoneId)], barrier_open: impl Fn(BarrierId) -> bool) -> bool {
    match gate {
        Gate::Open | Gate::OneWay => true,
        Gate::Door(a, b) => open_doors.iter().any(|&join| join == (a, b) || join == (b, a)),
        Gate::Barrier(barrier) => barrier_open(barrier),
    }
}

/// Seconds until a Zombie this far from the Player plans its route again.
pub fn repath_interval(distance: f32) -> f32 {
    if distance > ZOMBIE_REPATH_FAR_DIST {
        ZOMBIE_REPATH_FAR_SECS
    } else {
        ZOMBIE_REPATH_SECS
    }
}

/// How far along a straight walk from `from` to `to` (feet), measured flat, the floor first rises
/// more than `ZOMBIE_STEP_UP` from one sample to the next, or gives out: about where the face of
/// the Ledge is. `None` if it never does.
pub fn floor_ledge(spatial: &SpatialQuery, from: Vec3, to: Vec3, filter: &SpatialQueryFilter) -> Option<f32> {
    let flat = Vec2::new(to.x - from.x, to.z - from.z).length();
    let samples = (flat / ZOMBIE_FLOOR_SAMPLE).ceil().max(1.0) as usize;
    let spacing = flat / samples as f32;
    // Looking down from above the higher end, to below the lower one.
    let top = from.y.max(to.y) + 1.0;
    let reach = top - from.y.min(to.y) + 2.0;
    let mut floor = from.y;
    for i in 1..=samples {
        let at = from.lerp(to, i as f32 / samples as f32);
        let face = (i as f32 - 0.5) * spacing;
        let Some(hit) = spatial.cast_ray(Vec3::new(at.x, top, at.z), Dir3::NEG_Y, reach, true, filter) else {
            return Some(face);
        };
        let height = top - hit.distance;
        if height - floor > ZOMBIE_STEP_UP {
            return Some(face);
        }
        floor = height;
    }
    None
}

/// The Zone a point is in, or `None` outside the building.
pub fn zone_at(point: Vec3) -> Option<ZoneId> {
    ZoneId::ALL.into_iter().find(|zone| {
        let (lo, hi) = zone.volume();
        point.cmpge(lo).all() && point.cmple(hi).all()
    })
}



pub struct Node {
    /// Its name, for drawing the graph and for tests.
    #[cfg_attr(not(test), allow(dead_code))]
    pub name: &'static str,
    /// Where a Zombie's feet stand at the node.
    pub at: Vec3,
    /// The Zone the node is in, or `None` outside the building.
    pub zone: Option<ZoneId>,
    /// A Vent: only ever dropped from, so never anyone's nearest node.
    pub vent: bool,
}

pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub gate: Gate,
}

/// The Map's path-node graph.
#[derive(Resource)]
pub struct NavGraph {
    /// The Map it's the graph of.
    pub map: MapId,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// For each node, the edges a Zombie can leave it by.
    exits: Vec<Vec<usize>>,
}

impl NavGraph {
    /// Builds a Map's graph from its hand-listed nodes and edges.
    pub fn new(map: MapId) -> Self {
        let layout = map.layout();
        let nodes: Vec<_> = layout
            .nav_nodes
            .iter()
            .map(|&(name, x, y, z)| {
                let at = Vec3::new(x, y, z);
                Node {
                    name,
                    at,
                    zone: zone_at(at + Vec3::Y * 0.1),
                    vent: name.starts_with("vent_"),
                }
            })
            .collect();
        let index = |name: &str| {
            layout
                .nav_nodes
                .iter()
                .position(|(n, ..)| *n == name)
                .unwrap_or_else(|| panic!("no path node named {name}"))
        };
        let edges: Vec<_> = layout
            .nav_edges
            .iter()
            .map(|&(a, b, gate)| Edge {
                from: index(a),
                to: index(b),
                gate,
            })
            .collect();
        let mut exits = vec![Vec::new(); nodes.len()];
        for (i, edge) in edges.iter().enumerate() {
            exits[edge.from].push(i);
            if !edge.gate.one_way() {
                exits[edge.to].push(i);
            }
        }
        Self { map, nodes, edges, exits }
    }

    /// The index of the node named `name`.
    #[cfg(test)]
    pub fn node(&self, name: &str) -> Option<usize> {
        self.nodes.iter().position(|node| node.name == name)
    }

    /// The node a Zombie or the Player standing at `feet` belongs to: the closest in the same
    /// Zone (or outside, if they're outside), with height counting extra so floors don't mix.
    /// Vents are never it.
    pub fn nearest(&self, feet: Vec3) -> usize {
        self.ranked(feet).next().expect("the graph has nodes")
    }

    /// The nearest of the closest few nodes to `feet`, as `nearest` ranks them, that `reachable`
    /// accepts: the node a body there can really walk to, or be walked to from, with a crate or
    /// a Ledge in the way of a nearer one. `None` if none of them is.
    #[cfg(test)]
    pub fn nearest_where(&self, feet: Vec3, reachable: impl Fn(&Node) -> bool) -> Option<usize> {
        self.ranked(feet).take(NEAREST_TRIES).find(|&i| reachable(&self.nodes[i]))
    }

    /// Every one of the closest few nodes to `feet`, as `nearest` ranks them, that `reachable`
    /// accepts, each with how far it is from `feet`.
    pub fn near_where(&self, feet: Vec3, reachable: impl Fn(&Node) -> bool) -> Vec<(usize, f32)> {
        self.ranked(feet)
            .take(NEAREST_TRIES)
            .filter(|&i| reachable(&self.nodes[i]))
            .map(|i| (i, self.nodes[i].at.distance(feet)))
            .collect()
    }

    /// Every node but the Vents, nearest `feet` first: those in its Zone, then the rest.
    fn ranked(&self, feet: Vec3) -> impl Iterator<Item = usize> {
        let zone = zone_at(feet + Vec3::Y * 0.1);
        let cost = |node: &Node| {
            let d = node.at - feet;
            Vec2::new(d.x, d.z).length() + 3.0 * d.y.abs()
        };
        let mut ranked: Vec<(bool, f32, usize)> = (0..self.nodes.len())
            .filter(|&i| !self.nodes[i].vent)
            .map(|i| (self.nodes[i].zone != zone, cost(&self.nodes[i]), i))
            .collect();
        ranked.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
        ranked.into_iter().map(|(_, _, i)| i)
    }

    /// The shortest route from one node to another over the edges passable now, as a list of
    /// nodes from `from` to `to`; `None` if there is no way through.
    pub fn plan(
        &self,
        from: usize,
        to: usize,
        open_doors: &[(ZoneId, ZoneId)],
        barrier_open: impl Fn(BarrierId) -> bool,
    ) -> Option<Vec<usize>> {
        // A* needs a totally ordered cost, so distances are in whole centimetres.
        let cm = |a: usize, b: usize| (self.nodes[a].at.distance(self.nodes[b].at) * 100.0) as u32;
        let successors = |&node: &usize| {
            self.exits[node]
                .iter()
                .map(|&i| &self.edges[i])
                .filter(|edge| passable(edge.gate, open_doors, &barrier_open))
                .map(|edge| {
                    let next = if edge.from == node { edge.to } else { edge.from };
                    (next, cm(node, next))
                })
                .collect::<Vec<_>>()
        };
        astar(&from, successors, |&node| cm(node, to), |&node| node == to).map(|(path, _)| path)
    }

    /// The shortest route over the edges passable now from any of `starts` to any of `ends`, each
    /// given with how far it is to walk onto it (from the body) or off it (on to the target), as
    /// a list of nodes; `None` if there is no way through. A body between nodes sets off for
    /// whichever leads on best, not just the nearest: the nearest can lie behind it, and planning
    /// from it turns the body back until a step later another node is nearest and it turns again.
    pub fn plan_between(
        &self,
        starts: &[(usize, f32)],
        ends: &[(usize, f32)],
        open_doors: &[(ZoneId, ZoneId)],
        barrier_open: impl Fn(BarrierId) -> bool,
    ) -> Option<Vec<usize>> {
        // Stand-ins for the body and its target, joined to the nodes they can walk to and from.
        const BODY: usize = usize::MAX;
        const TARGET: usize = usize::MAX - 1;
        let cm = |metres: f32| (metres * 100.0) as u32;
        let successors = |&node: &usize| -> Vec<(usize, u32)> {
            match node {
                BODY => starts.iter().map(|&(i, walk)| (i, cm(walk))).collect(),
                TARGET => Vec::new(),
                _ => self.exits[node]
                    .iter()
                    .map(|&i| &self.edges[i])
                    .filter(|edge| passable(edge.gate, open_doors, &barrier_open))
                    .map(|edge| {
                        let next = if edge.from == node { edge.to } else { edge.from };
                        (next, cm(self.nodes[node].at.distance(self.nodes[next].at)))
                    })
                    .chain(ends.iter().filter(|&&(end, _)| end == node).map(|&(_, walk)| (TARGET, cm(walk))))
                    .collect(),
            }
        };
        let heuristic = |&node: &usize| match node {
            BODY | TARGET => 0,
            _ => ends
                .iter()
                .map(|&(end, walk)| cm(self.nodes[node].at.distance(self.nodes[end].at) + walk))
                .min()
                .unwrap_or(0),
        };
        let (path, _) = astar(&BODY, successors, heuristic, |&node| node == TARGET)?;
        Some(path.into_iter().filter(|&node| node != BODY && node != TARGET).collect())
    }
}

/// A Zombie's current route to the Player it's going after.
#[derive(Component, Default)]
pub struct Route {
    /// The Player it's going after, picked every `ZOMBIE_RETARGET_SECS`. `None` until it picks.
    pub target: Option<Entity>,
    /// Seconds until it picks again.
    pub retarget_in: f32,
    /// Path nodes still to reach, nearest first. Empty means head straight for the Player.
    pub waypoints: Vec<Vec3>,
    /// Seconds until the next re-plan.
    pub repath_in: f32,
}

#[derive(Default, Reflect, GizmoConfigGroup)]
struct NavGizmos;

pub struct NavPlugin;

impl Plugin for NavPlugin {
    fn build(&self, app: &mut App) {
        app.init_gizmo_group::<NavGizmos>()
            .insert_resource(NavGraph::new(MapId::default()))
            .add_systems(OnEnter(GameState::Playing), build_graph)
            .add_systems(
                Update,
                draw_graph
                    .run_if(in_state(GameState::Playing))
                    .run_if(|view: Res<DebugView>| view.0),
            );
    }
}

/// The graph of the Map the Run is played on.
pub fn build_graph(map: Res<MapId>, mut graph: ResMut<NavGraph>) {
    if graph.map != *map {
        *graph = NavGraph::new(*map);
    }
}

/// F1 view: open edges grey, shut Doors red, open Doors green, planked Barriers yellow, open
/// Barriers as green arrows, one-way edges as blue arrows; each Zombie's route in orange.
fn draw_graph(
    mut gizmos: Gizmos<NavGizmos>,
    graph: Res<NavGraph>,
    doors: Query<&Door>,
    barriers: Res<Barriers>,
    routes: Query<(&Transform, &Route)>,
) {
    let open_doors: Vec<_> = doors.iter().filter(|door| door.open).filter_map(|door| door.joins).collect();
    let lift = Vec3::Y * 0.3;
    for node in &graph.nodes {
        gizmos.sphere(Isometry3d::from_translation(node.at + lift), 0.15, Color::WHITE);
    }
    for edge in &graph.edges {
        let (a, b) = (graph.nodes[edge.from].at + lift, graph.nodes[edge.to].at + lift);
        let colour = match edge.gate {
            Gate::Open => Color::srgb(0.7, 0.7, 0.7),
            Gate::Door(..) | Gate::Barrier(_) if passable(edge.gate, &open_doors, |b| barriers.is_open(b)) => {
                Color::srgb(0.2, 1.0, 0.3)
            }
            Gate::Door(..) => Color::srgb(1.0, 0.2, 0.2),
            Gate::Barrier(_) => Color::srgb(1.0, 0.9, 0.2),
            Gate::OneWay => Color::srgb(0.3, 0.6, 1.0),
        };
        if edge.gate.one_way() {
            gizmos.arrow(a, b, colour);
        } else {
            gizmos.line(a, b, colour);
        }
    }
    let orange = Color::srgb(1.0, 0.55, 0.1);
    for (transform, route) in &routes {
        let mut from = transform.translation;
        for &point in &route.waypoints {
            gizmos.line(from, point + lift, orange);
            from = point + lift;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::GameLayer;
    use crate::maps::map::{WINDOWS, window_spawn};
    use crate::enemies::zombie::headless::Sim;
    use ZoneId::*;

    /// Whether the floor along a straight walk from `from` to `to` (feet) never rises more than a
    /// step at a time (see `floor_ledge`).
    fn floor_climbable(spatial: &SpatialQuery, from: Vec3, to: Vec3, filter: &SpatialQueryFilter) -> bool {
        floor_ledge(spatial, from, to, filter).is_none()
    }

    /// Every window stripped of its Planks.
    fn windows_open(_: BarrierId) -> bool {
        true
    }

    /// Every Door on `map` bought: every pair of its Zones, a Zone with itself for a Door inside one.
    fn all_doors(map: MapId) -> Vec<(ZoneId, ZoneId)> {
        let zones = map.layout().zones;
        zones.iter().flat_map(|&a| zones.iter().map(move |&b| (a, b))).collect()
    }

    fn greybox() -> NavGraph {
        NavGraph::new(MapId::Greybox)
    }

    fn node(name: &str) -> usize {
        greybox().node(name).unwrap()
    }

    fn route(from: &str, to: &str, open_doors: &[(ZoneId, ZoneId)]) -> Option<Vec<&'static str>> {
        let graph = greybox();
        let path = graph.plan(node(from), node(to), open_doors, windows_open)?;
        Some(path.into_iter().map(|i| graph.nodes[i].name).collect())
    }

    #[test]
    fn every_walked_edge_is_a_clear_walk_through_the_built_map() {
        use bevy::ecs::system::RunSystemOnce;

        for map in MapId::ALL {
            let mut sim = Sim::on(map, 60.0, |_| {});
            let blocked: Vec<String> = sim
                .world_mut()
                .run_system_once(move |spatial: SpatialQuery, doors: Query<Entity, With<Door>>| {
                    let graph = NavGraph::new(map);
                    // Doors stand open once bought; the Vents and the drops are falls, and a window
                    // is climbed over its Sill, not walked.
                    let filter = SpatialQueryFilter::from_mask(GameLayer::World).with_excluded_entities(doors.iter());
                    let body = Collider::sphere(ZOMBIE_RADIUS * 0.8);
                    graph
                        .edges
                        .iter()
                        .filter(|edge| !matches!(edge.gate, Gate::OneWay | Gate::Barrier(_)))
                        .filter_map(|edge| {
                            let (from, to) = (graph.nodes[edge.from].at + Vec3::Y, graph.nodes[edge.to].at + Vec3::Y);
                            let dir = Dir3::new(to - from).ok()?;
                            let config = ShapeCastConfig::from_max_distance(from.distance(to));
                            spatial
                                .cast_shape(&body, from, Quat::IDENTITY, dir, &config, &filter)
                                .map(|_| format!("{} -> {}", graph.nodes[edge.from].name, graph.nodes[edge.to].name))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap();
            assert!(blocked.is_empty(), "blocked on the {}: {blocked:?}", map.name());
        }
    }

    /// A two-way edge is walked both ways: its floor never rises more than a step either way, so
    /// Raised Floors are reached by their steps and ramps, never up a Ledge.
    #[test]
    fn no_two_way_edge_climbs_a_ledge() {
        use bevy::ecs::system::RunSystemOnce;

        for map in MapId::ALL {
            let mut sim = Sim::on(map, 60.0, |_| {});
            let ledges: Vec<String> = sim
                .world_mut()
                .run_system_once(move |spatial: SpatialQuery, doors: Query<Entity, With<Door>>| {
                    let graph = NavGraph::new(map);
                    // Doors stand open once bought.
                    let filter = SpatialQueryFilter::from_mask(GameLayer::World).with_excluded_entities(doors.iter());
                    graph
                        .edges
                        .iter()
                        .filter(|edge| !edge.gate.one_way())
                        .flat_map(|edge| [(edge.from, edge.to), (edge.to, edge.from)])
                        .filter(|&(a, b)| !floor_climbable(&spatial, graph.nodes[a].at, graph.nodes[b].at, &filter))
                        .map(|(a, b)| format!("{} -> {}", graph.nodes[a].name, graph.nodes[b].name))
                        .collect::<Vec<_>>()
                })
                .unwrap();
            assert!(ledges.is_empty(), "climbs a ledge on the {}: {ledges:?}", map.name());
        }
    }

    #[test]
    fn open_and_one_way_edges_are_always_passable() {
        assert!(passable(Gate::Open, &[], windows_open));
        assert!(passable(Gate::OneWay, &[], windows_open));
    }

    #[test]
    fn a_door_edge_is_passable_only_once_that_door_is_bought() {
        let gate = Gate::Door(StartRoom, WestHall);
        assert!(!passable(gate, &[], windows_open));
        assert!(!passable(gate, &[(StartRoom, EastHall)], windows_open));
        assert!(passable(gate, &[(StartRoom, WestHall)], windows_open));
        assert!(passable(gate, &[(WestHall, StartRoom)], windows_open), "Doors join both ways");
    }

    #[test]
    fn a_barrier_edge_follows_its_barrier() {
        let gate = Gate::Barrier(BarrierId(3));
        assert!(passable(gate, &[], windows_open));
        assert!(!passable(gate, &[], |b| b != BarrierId(3)));
        assert!(passable(gate, &[], |b| b == BarrierId(3)));
    }

    #[test]
    fn every_map_has_a_graph_of_its_own() {
        for map in MapId::ALL {
            let graph = NavGraph::new(map);
            assert_eq!(graph.edges.len(), map.layout().nav_edges.len());
            assert!((40..=200).contains(&graph.nodes.len()), "the {} has {} nodes", map.name(), graph.nodes.len());
            for (i, n) in graph.nodes.iter().enumerate() {
                assert_eq!(graph.node(n.name), Some(i), "{} is listed twice", n.name);
                assert!(n.zone.is_none_or(|zone| zone.map() == map), "{} is in another Map's Zone", n.name);
            }
        }
    }

    #[test]
    fn every_window_has_one_barrier_edge_from_its_spawn_point_into_its_zone() {
        for map in MapId::ALL {
            let graph = NavGraph::new(map);
            for barrier in map.barriers() {
                let (zone, ..) = WINDOWS[barrier.0 as usize];
                let edges: Vec<_> = graph.edges.iter().filter(|e| e.gate == Gate::Barrier(barrier)).collect();
                assert_eq!(edges.len(), 1, "window {}", barrier.0);
                let (outside, inside) = (&graph.nodes[edges[0].from], &graph.nodes[edges[0].to]);
                assert!(outside.at.distance(window_spawn(barrier)) < 0.01, "window {}", barrier.0);
                assert_eq!(outside.zone, None, "window {}", barrier.0);
                assert_eq!(inside.zone, Some(zone), "window {}", barrier.0);
            }
            assert!(graph.edges.iter().all(|edge| !matches!(edge.gate, Gate::Barrier(b) if WINDOWS[b.0 as usize].0.map() != map)));
        }
    }

    #[test]
    fn only_window_spawn_points_lie_outside() {
        for map in MapId::ALL {
            for n in &NavGraph::new(map).nodes {
                assert_eq!(n.zone.is_none(), n.name.ends_with("_out"), "{}", n.name);
            }
        }
    }

    #[test]
    fn with_every_door_bought_every_node_reaches_every_indoor_node() {
        for map in MapId::ALL {
            let graph = NavGraph::new(map);
            let doors = all_doors(map);
            for from in 0..graph.nodes.len() {
                for to in (0..graph.nodes.len()).filter(|&to| graph.nodes[to].zone.is_some() && !graph.nodes[to].vent) {
                    assert!(graph.plan(from, to, &doors, windows_open).is_some(), "{} to {}", graph.nodes[from].name, graph.nodes[to].name);
                }
            }
        }
    }

    /// Just west of the Quarry's `head_s`, `head_w` is the nearest node, but it only leads back by
    /// `head_s`: a body there bound up the Headframe's stairs sets off straight for the corner by
    /// the Power Door instead.
    #[test]
    fn a_route_sets_off_from_whichever_near_node_leads_on_best() {
        let graph = NavGraph::new(MapId::Quarry);
        let named = |name: &str| graph.node(name).unwrap();
        let feet = Vec3::new(crate::maps::map::QUARRY_X, 2.0 * crate::maps::map::STOREY, 29.1);
        assert_eq!(graph.nearest(feet), named("head_w"));
        // Anything on the yard's floor is a clear walk from there; the stairs are behind a wall.
        let from = graph.near_where(feet, |node| node.at.y == feet.y);
        let path = graph.plan_between(&from, &[(named("stair_mid"), 0.0)], &all_doors(MapId::Quarry), windows_open).unwrap();
        let names: Vec<_> = path.iter().map(|&i| graph.nodes[i].name).collect();
        assert_eq!(names, ["head_corner", "stair_low", "stair_mid"]);
    }

    /// The Village's lanes are three dead ends until the Power opens the gates either end of the
    /// Back Lane. Then a Zombie in the Back Lane can walk out along either lane.
    #[test]
    fn the_villages_lanes_join_up_only_with_the_power() {
        let graph = NavGraph::new(MapId::Village);
        let named = |name: &str| graph.node(name).unwrap();
        // Every Door bought, but the Power's gates are not bought: they open with the Power.
        let gates = [(BackLane, MillLane), (BackLane, ChurchLane)];
        let gated = |&(a, b): &(ZoneId, ZoneId)| gates.contains(&(a, b)) || gates.contains(&(b, a));
        let bought: Vec<_> = all_doors(MapId::Village).into_iter().filter(|join| !gated(join)).collect();
        for lane in ["ml_c", "cl_c"] {
            // The long way round, down through a building, along the High Street and back up.
            let around = graph.plan(named("bl_c"), named(lane), &bought, windows_open).expect("the long way round");
            assert!(around.iter().any(|&i| graph.nodes[i].zone == Some(Market)), "to {lane} without the Power: {around:?}");
            let gated: Vec<_> = bought.iter().copied().chain(gates).collect();
            let through = graph.plan(named("bl_c"), named(lane), &gated, windows_open).unwrap();
            assert!(through.iter().all(|&i| matches!(graph.nodes[i].zone, Some(BackLane | MillLane | ChurchLane))), "to {lane} with the Power: {through:?}");
        }
    }

    #[test]
    fn shut_doors_block_zombies() {
        assert_eq!(route("start", "west_ne", &[]), None);
        assert!(route("start", "west_ne", &[(StartRoom, WestHall)]).is_some());
    }

    #[test]
    fn closed_barriers_block_zombies() {
        let graph = greybox();
        assert!(graph.plan(node("w0_out"), node("start"), &[], |_| false).is_none());
        assert!(graph.plan(node("w0_out"), node("start"), &[], windows_open).is_some());
    }

    #[test]
    fn zombies_never_climb_back_out_of_a_window() {
        assert_eq!(route("start", "w0_out", &all_doors(MapId::Greybox)), None);
    }

    #[test]
    fn west_hall_reaches_the_yard_up_the_ramp_and_down_the_drop() {
        let path = route("west_ne", "yard_c", &[]).expect("the drop is always open");
        assert!(path.contains(&"ramp_mid"));
        assert!(path.windows(2).any(|w| w == ["drop_top", "drop_bottom"]));
    }

    #[test]
    fn the_drop_is_one_way() {
        assert_eq!(route("yard_c", "bal_w", &[]), None);
        // With the Start Room's Doors bought, the way up is round through West Hall and the ramp.
        let path = route("yard_c", "bal_w", &[(StartRoom, Yard), (StartRoom, WestHall)]).unwrap();
        assert!(path.contains(&"start") && path.contains(&"ramp_top"));
    }

    #[test]
    fn a_far_route_goes_round_the_ring() {
        // East Hall's Door from the Start Room is shut, so the way there is round by the Power Room.
        let open = [(StartRoom, WestHall), (WestHall, DiningRoom), (DiningRoom, PowerRoom), (EastHall, PowerRoom)];
        let path = route("start", "east_ne", &open).unwrap();
        let graph = greybox();
        let zones: Vec<_> = path.iter().filter_map(|name| graph.nodes[node(name)].zone).collect();
        for on_the_way in [WestHall, DiningRoom, PowerRoom] {
            assert!(zones.contains(&on_the_way), "{path:?}");
        }
        assert!(path.contains(&"east_pd"), "{path:?}");
    }

    #[test]
    fn the_nearest_node_stays_on_the_same_floor_and_zone() {
        let graph = greybox();
        let name = |at: Vec3| graph.nodes[graph.nearest(at)].name;
        assert_eq!(name(Vec3::new(1.0, 0.0, 19.0)), "start");
        assert_eq!(name(Vec3::new(-12.0, 4.0, 3.0)), "drop_top");
        // Right under the drop, in the Yard.
        assert_eq!(name(Vec3::new(-9.5, 0.0, 4.0)), "drop_bottom");
        // Halfway up the ramp.
        assert_eq!(name(Vec3::new(-15.7, 2.0, 4.0)), "ramp_mid");
        assert_eq!(name(Vec3::new(-28.0, 0.0, 4.5)), "w3_out");
        // Just inside the Start Room's side of the Start–Yard Door, not the Yard's.
        assert_eq!(name(Vec3::new(0.0, 0.0, 14.5)), "start_s");
    }

    #[test]
    fn the_nearest_reachable_node_passes_over_ones_that_cannot_be_reached() {
        let graph = greybox();
        let at = Vec3::new(1.0, 0.0, 19.0);
        let start = node("start");
        assert_eq!(graph.nearest_where(at, |_| true), Some(start));
        // With the nearest out of reach, the next nearest in the same Zone.
        let next = graph.nearest_where(at, |n| n.at != graph.nodes[start].at).unwrap();
        assert_ne!(next, start);
        assert_eq!(graph.nodes[next].zone, Some(StartRoom));
        assert_eq!(graph.nearest_where(at, |_| false), None);
    }

    #[test]
    fn far_zombies_replan_less_often() {
        assert_eq!(repath_interval(5.0), ZOMBIE_REPATH_SECS);
        assert_eq!(repath_interval(30.0), ZOMBIE_REPATH_SECS);
        assert_eq!(repath_interval(30.5), ZOMBIE_REPATH_FAR_SECS);
    }

    #[test]
    fn every_vent_is_a_one_way_drop_onto_the_spot_below_it() {
        for map in MapId::ALL {
            let graph = NavGraph::new(map);
            for &(zone, floor) in map.layout().vents {
                let vent = graph.nodes.iter().position(|n| n.vent && n.at == floor + Vec3::Y * NOVA_VENT_HEIGHT).unwrap();
                assert_eq!(graph.nodes[vent].zone, Some(zone));
                let edges: Vec<_> = graph.edges.iter().filter(|e| e.from == vent || e.to == vent).collect();
                assert_eq!(edges.len(), 1, "{}", graph.nodes[vent].name);
                assert_eq!((edges[0].from, edges[0].gate), (vent, Gate::OneWay));
                assert_eq!(graph.nodes[edges[0].to].at, floor);
                // Nothing climbs back up into it.
                assert!(graph.plan(edges[0].to, vent, &all_doors(map), windows_open).is_none());
            }
        }
    }

    #[test]
    fn a_vent_is_never_the_nearest_node() {
        let graph = greybox();
        // Up by the Dining Room's Vent, as a Player mid-jump might be.
        assert_eq!(graph.nodes[graph.nearest(Vec3::new(-13.0, 3.0, -20.0))].name, "dining_c");
    }
}
