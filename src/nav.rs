//! Zombie navigation: a hand-authored graph of path nodes, planned over with A* (ADR 0001).

use bevy::prelude::*;
use pathfinding::prelude::astar;

use crate::barrier::Barriers;
use crate::debug::DebugView;
use crate::door::Door;
use crate::map::BarrierId;
use crate::state::GameState;
use crate::tuning::*;
use crate::zone::ZoneId;

/// What gates an edge of the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// Always open: across a room, or up and down the ramp.
    Open,
    /// Through the Door between two Zones: open once it's bought.
    Door(ZoneId, ZoneId),
    /// In through a window, from outside only: open when no Planks are left.
    Barrier(BarrierId),
    /// From the first node to the second only: the drop off the Balcony (and later the Vents).
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

/// The Zone a point is in, or `None` outside the building.
pub fn zone_at(point: Vec3) -> Option<ZoneId> {
    ZoneId::ALL.into_iter().find(|zone| {
        let (lo, hi) = zone.volume();
        point.cmpge(lo).all() && point.cmple(hi).all()
    })
}

/// The path nodes: a name, then where a Zombie's feet stand there (x, y, z).
const NODES: &[(&str, f32, f32, f32)] = &[
    // Start Room
    ("start", 0.0, 0.0, 20.0),
    ("start_w", -12.5, 0.0, 20.0),
    ("start_e", 12.5, 0.0, 20.0),
    ("start_s", 0.0, 0.0, 15.5),
    ("w0_in", -7.0, 0.0, 24.5),
    ("w0_out", -7.0, 0.0, 28.0),
    ("w1_in", 7.0, 0.0, 24.5),
    ("w1_out", 7.0, 0.0, 28.0),
    // West Hall, its ramp and the Balcony
    ("west_e", -15.5, 0.0, 20.0),
    ("west_ne", -20.0, 0.0, 20.0),
    ("west_mid", -21.0, 0.0, 4.0),
    ("west_s", -20.0, 0.0, -6.0),
    ("west_dd", -20.0, 0.0, -12.5),
    ("ramp_bottom", -15.7, 0.0, 10.0),
    ("ramp_mid", -15.7, 2.0, 4.0),
    ("ramp_top", -15.7, 4.0, -2.0),
    ("w2_in", -24.5, 0.0, 18.0),
    ("w2_out", -28.0, 0.0, 18.0),
    ("w3_in", -24.5, 0.0, 4.0),
    ("w3_out", -28.0, 0.0, 4.0),
    ("w4_in", -24.5, 0.0, -8.0),
    ("w4_out", -28.0, 0.0, -8.0),
    ("bal_w", -12.5, 4.0, -2.0),
    ("drop_top", -11.0, 4.0, 4.0),
    // Yard
    ("drop_bottom", -8.5, 0.0, 4.0),
    ("yard_n", 0.0, 0.0, 12.5),
    ("yard_c", 0.0, 0.0, 0.0),
    ("yard_w", -8.0, 0.0, 0.0),
    ("yard_e", 11.0, 0.0, -2.0),
    ("yard_sd", -7.0, 0.0, -12.5),
    // East Hall
    ("east_w", 15.5, 0.0, 20.0),
    ("east_ne", 20.0, 0.0, 20.0),
    ("east_mid", 21.0, 0.0, 4.0),
    ("east_s", 20.0, 0.0, -6.0),
    ("east_pd", 20.0, 0.0, -12.5),
    ("w5_in", 24.5, 0.0, 18.0),
    ("w5_out", 28.0, 0.0, 18.0),
    ("w6_in", 24.5, 0.0, 4.0),
    ("w6_out", 28.0, 0.0, 4.0),
    ("w7_in", 24.5, 0.0, -8.0),
    ("w7_out", 28.0, 0.0, -8.0),
    // Dining Room
    ("dining_c", -13.0, 0.0, -20.0),
    ("dining_wn", -20.0, 0.0, -15.5),
    ("dining_yn", -7.0, 0.0, -15.5),
    ("dining_pe", -1.5, 0.0, -20.0),
    ("w8_in", -18.0, 0.0, -24.5),
    ("w8_out", -18.0, 0.0, -28.0),
    ("w9_in", -6.0, 0.0, -24.5),
    ("w9_out", -6.0, 0.0, -28.0),
    ("w10_in", -24.5, 0.0, -20.0),
    ("w10_out", -28.0, 0.0, -20.0),
    // Power Room
    ("power_c", 13.0, 0.0, -20.0),
    ("power_w", 1.5, 0.0, -20.0),
    ("power_n", 20.0, 0.0, -15.5),
    ("w11_in", 8.0, 0.0, -24.5),
    ("w11_out", 8.0, 0.0, -28.0),
    ("w12_in", 20.0, 0.0, -24.5),
    ("w12_out", 20.0, 0.0, -28.0),
];

const fn door(a: ZoneId, b: ZoneId) -> Gate {
    Gate::Door(a, b)
}

const fn barrier(id: u8) -> Gate {
    Gate::Barrier(BarrierId(id))
}

const OPEN: Gate = Gate::Open;

/// The edges, by node name. Barrier and one-way edges run from the first node to the second.
/// Every edge is a straight walk clear of walls and Yard cover.
const EDGES: &[(&str, &str, Gate)] = {
    use ZoneId::*;
    &[
        // Start Room
        ("start", "start_w", OPEN),
        ("start", "start_e", OPEN),
        ("start", "start_s", OPEN),
        ("start", "w0_in", OPEN),
        ("start", "w1_in", OPEN),
        ("w0_out", "w0_in", barrier(0)),
        ("w1_out", "w1_in", barrier(1)),
        ("start_w", "west_e", door(StartRoom, WestHall)),
        ("start_e", "east_w", door(StartRoom, EastHall)),
        ("start_s", "yard_n", door(StartRoom, Yard)),
        // West Hall
        ("west_e", "west_ne", OPEN),
        ("west_e", "ramp_bottom", OPEN),
        ("west_ne", "ramp_bottom", OPEN),
        ("west_ne", "west_mid", OPEN),
        ("west_mid", "west_s", OPEN),
        ("west_s", "west_dd", OPEN),
        ("west_ne", "w2_in", OPEN),
        ("west_mid", "w3_in", OPEN),
        ("west_s", "w4_in", OPEN),
        ("w2_out", "w2_in", barrier(2)),
        ("w3_out", "w3_in", barrier(3)),
        ("w4_out", "w4_in", barrier(4)),
        ("west_dd", "dining_wn", door(WestHall, DiningRoom)),
        // Up the ramp, along the Balcony and down the drop
        ("ramp_bottom", "ramp_mid", OPEN),
        ("ramp_mid", "ramp_top", OPEN),
        ("ramp_top", "bal_w", OPEN),
        ("bal_w", "drop_top", OPEN),
        ("drop_top", "drop_bottom", Gate::OneWay),
        // Yard
        ("drop_bottom", "yard_w", OPEN),
        ("drop_bottom", "yard_n", OPEN),
        ("yard_w", "yard_c", OPEN),
        ("yard_c", "yard_n", OPEN),
        ("yard_c", "yard_e", OPEN),
        ("yard_w", "yard_sd", OPEN),
        ("yard_sd", "dining_yn", door(Yard, DiningRoom)),
        // East Hall
        ("east_w", "east_ne", OPEN),
        ("east_ne", "east_mid", OPEN),
        ("east_mid", "east_s", OPEN),
        ("east_s", "east_pd", OPEN),
        ("east_ne", "w5_in", OPEN),
        ("east_mid", "w6_in", OPEN),
        ("east_s", "w7_in", OPEN),
        ("w5_out", "w5_in", barrier(5)),
        ("w6_out", "w6_in", barrier(6)),
        ("w7_out", "w7_in", barrier(7)),
        ("east_pd", "power_n", door(EastHall, PowerRoom)),
        // Dining Room
        ("dining_c", "dining_wn", OPEN),
        ("dining_c", "dining_yn", OPEN),
        ("dining_c", "dining_pe", OPEN),
        ("dining_c", "w8_in", OPEN),
        ("dining_c", "w9_in", OPEN),
        ("dining_c", "w10_in", OPEN),
        ("w8_out", "w8_in", barrier(8)),
        ("w9_out", "w9_in", barrier(9)),
        ("w10_out", "w10_in", barrier(10)),
        ("dining_pe", "power_w", door(DiningRoom, PowerRoom)),
        // Power Room
        ("power_c", "power_w", OPEN),
        ("power_c", "power_n", OPEN),
        ("power_c", "w11_in", OPEN),
        ("power_c", "w12_in", OPEN),
        ("w11_out", "w11_in", barrier(11)),
        ("w12_out", "w12_in", barrier(12)),
    ]
};

pub struct Node {
    /// Where a Zombie's feet stand at the node.
    pub at: Vec3,
    /// The Zone the node is in, or `None` outside the building.
    pub zone: Option<ZoneId>,
}

pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub gate: Gate,
}

/// The Map's path-node graph.
#[derive(Resource)]
pub struct NavGraph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// For each node, the edges a Zombie can leave it by.
    exits: Vec<Vec<usize>>,
}

impl NavGraph {
    /// Builds the graph from the hand-listed nodes and edges.
    pub fn new() -> Self {
        let nodes: Vec<_> = NODES
            .iter()
            .map(|&(_, x, y, z)| {
                let at = Vec3::new(x, y, z);
                Node {
                    at,
                    zone: zone_at(at + Vec3::Y * 0.1),
                }
            })
            .collect();
        let index = |name: &str| {
            NODES
                .iter()
                .position(|(n, ..)| *n == name)
                .unwrap_or_else(|| panic!("no path node named {name}"))
        };
        let edges: Vec<_> = EDGES
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
        Self { nodes, edges, exits }
    }

    /// The node a Zombie or the Player standing at `feet` belongs to: the closest in the same
    /// Zone (or outside, if they're outside), with height counting extra so floors don't mix.
    pub fn nearest(&self, feet: Vec3) -> usize {
        let zone = zone_at(feet + Vec3::Y * 0.1);
        let cost = |node: &Node| {
            let d = node.at - feet;
            Vec2::new(d.x, d.z).length() + 3.0 * d.y.abs()
        };
        let pick = |same_zone: bool| {
            (0..self.nodes.len())
                .filter(|&i| !same_zone || self.nodes[i].zone == zone)
                .min_by(|&a, &b| cost(&self.nodes[a]).total_cmp(&cost(&self.nodes[b])))
        };
        pick(true).or_else(|| pick(false)).expect("the graph has nodes")
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
}

/// A Zombie's current route to the Player.
#[derive(Component, Default)]
pub struct Route {
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
            .insert_resource(NavGraph::new())
            .add_systems(
                Update,
                draw_graph
                    .run_if(in_state(GameState::Playing))
                    .run_if(|view: Res<DebugView>| view.0),
            );
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
    use crate::map::{WINDOWS, window_spawn};
    use ZoneId::*;

    /// Every window stripped of its Planks.
    fn windows_open(_: BarrierId) -> bool {
        true
    }

    fn node(name: &str) -> usize {
        NODES.iter().position(|(n, ..)| *n == name).unwrap()
    }

    fn route(from: &str, to: &str, open_doors: &[(ZoneId, ZoneId)]) -> Option<Vec<&'static str>> {
        let graph = NavGraph::new();
        let path = graph.plan(node(from), node(to), open_doors, windows_open)?;
        Some(path.into_iter().map(|i| NODES[i].0).collect())
    }

    const ALL_DOORS: [(ZoneId, ZoneId); 7] = [
        (StartRoom, WestHall),
        (StartRoom, EastHall),
        (StartRoom, Yard),
        (WestHall, DiningRoom),
        (EastHall, PowerRoom),
        (DiningRoom, PowerRoom),
        (Yard, DiningRoom),
    ];

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
    fn every_edge_names_real_nodes() {
        let graph = NavGraph::new();
        assert_eq!(graph.edges.len(), EDGES.len());
        assert!((40..=60).contains(&graph.nodes.len()), "about 40 nodes, not {}", graph.nodes.len());
    }

    #[test]
    fn node_names_are_unique() {
        for (i, (name, ..)) in NODES.iter().enumerate() {
            assert_eq!(node(name), i, "{name} is listed twice");
        }
    }

    #[test]
    fn every_window_has_one_barrier_edge_from_its_spawn_point_into_its_zone() {
        let graph = NavGraph::new();
        for (i, (zone, ..)) in WINDOWS.iter().enumerate() {
            let barrier = BarrierId(i as u8);
            let edges: Vec<_> = graph.edges.iter().filter(|e| e.gate == Gate::Barrier(barrier)).collect();
            assert_eq!(edges.len(), 1, "window {i}");
            let (outside, inside) = (&graph.nodes[edges[0].from], &graph.nodes[edges[0].to]);
            assert!(outside.at.distance(window_spawn(barrier)) < 0.01, "window {i}");
            assert_eq!(outside.zone, None, "window {i}");
            assert_eq!(inside.zone, Some(*zone), "window {i}");
        }
    }

    #[test]
    fn only_window_spawn_points_lie_outside() {
        let graph = NavGraph::new();
        for (i, n) in graph.nodes.iter().enumerate() {
            assert_eq!(n.zone.is_none(), NODES[i].0.ends_with("_out"), "{}", NODES[i].0);
        }
    }

    #[test]
    fn with_every_door_bought_every_node_reaches_every_indoor_node() {
        let graph = NavGraph::new();
        for (from, (from_name, ..)) in NODES.iter().enumerate() {
            for to in (0..graph.nodes.len()).filter(|&to| graph.nodes[to].zone.is_some()) {
                assert!(
                    graph.plan(from, to, &ALL_DOORS, windows_open).is_some(),
                    "{from_name} to {}",
                    NODES[to].0
                );
            }
        }
    }

    #[test]
    fn shut_doors_block_zombies() {
        assert_eq!(route("start", "west_ne", &[]), None);
        assert!(route("start", "west_ne", &[(StartRoom, WestHall)]).is_some());
    }

    #[test]
    fn closed_barriers_block_zombies() {
        let graph = NavGraph::new();
        assert!(graph.plan(node("w0_out"), node("start"), &[], |_| false).is_none());
        assert!(graph.plan(node("w0_out"), node("start"), &[], windows_open).is_some());
    }

    #[test]
    fn zombies_never_climb_back_out_of_a_window() {
        assert_eq!(route("start", "w0_out", &ALL_DOORS), None);
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
        for on_the_way in ["west_e", "dining_c", "power_c", "east_pd"] {
            assert!(path.contains(&on_the_way), "{path:?}");
        }
    }

    #[test]
    fn the_nearest_node_stays_on_the_same_floor_and_zone() {
        let graph = NavGraph::new();
        let name = |at: Vec3| NODES[graph.nearest(at)].0;
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
    fn far_zombies_replan_less_often() {
        assert_eq!(repath_interval(5.0), ZOMBIE_REPATH_SECS);
        assert_eq!(repath_interval(30.0), ZOMBIE_REPATH_SECS);
        assert_eq!(repath_interval(30.5), ZOMBIE_REPATH_FAR_SECS);
    }
}
