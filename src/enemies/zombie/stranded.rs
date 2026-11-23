//! Stranded Zombies: what a Quarantine seal does to a Zombie it leaves with no way to any Player.
//!
//! A Zombie that has had no route to any Player it could go after for `QUARANTINE_STRAND_SECS`, while a
//! Wing is Sealed or Sealing, is Stranded. It walks to the nearest shut Quarantine Door on its side,
//! pounds on it for `QUARANTINE_POUND_SECS` (the attack swing, with `Cue::DoorPound` at each blow), and
//! then goes back into the Horde the way a stuck Zombie does. A special or the Sapper is moved to a
//! fresh Special Spawn Point instead, and keeps its place. A Zombie that can still reach some Player is
//! never Stranded, and one whose way opens again (the doors swing open, a Player pushes a Crash Bar and
//! steps outside) goes back to the chase.
//!
//! The Host decides all of it. The pound is the swing a Zombie already makes, so every machine sees it
//! in the snapshot, and the cue is told with the other shared ones.

use super::*;
use crate::audio::PlayCue;
use crate::enemies::special::special_spawn_candidates;
use crate::enemies::target::PlayerSpot;
use crate::maps::map::{SpawnKind, SpawnPoint};
use crate::maps::quarantine::{Doorway, Quarantine};
use crate::maps::shifting::{PieceId, PieceState, pieces, standing_rests};
use crate::maps::zone::{ActiveZones, ZoneId};
use crate::players::player::Player;

/// Watches a Zombie for being cut off by a seal.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct CutOff {
    /// Seconds with no route to any Player, while a Wing is Sealed or Sealing.
    secs: f32,
    /// Seconds until its route is looked for again.
    check_in: f32,
    /// What the last look found.
    reachable: bool,
}

impl Default for CutOff {
    fn default() -> Self {
        Self { secs: 0.0, check_in: 0.0, reachable: true }
    }
}

impl CutOff {
    /// Runs `dt` seconds on. `reachable` is whether it has a route to some Player (as of the last look),
    /// and `sealing` whether a Wing is Sealed or Sealing. Returns whether it is Stranded now.
    pub fn advance(&mut self, reachable: bool, sealing: bool, dt: f32) -> bool {
        self.reachable = reachable;
        self.secs = if reachable || !sealing { 0.0 } else { self.secs + dt };
        self.secs >= QUARANTINE_STRAND_SECS
    }

    fn reset(&mut self) {
        self.secs = 0.0;
    }
}

/// Whether a Zombie at `from` (its feet) is cut off from every Player it could go after: some Player
/// is up, and no route over the path nodes leads to any of them. With no Player up it is not cut off:
/// there is nobody to be cut off from.
pub fn cut_off(graph: &NavGraph, from: Vec3, players: impl IntoIterator<Item = PlayerSpot>, state: NavState) -> bool {
    let up: Vec<PlayerSpot> = players.into_iter().filter(|player| player.up).collect();
    !up.is_empty() && up.iter().all(|player| crate::enemies::target::path_distance(graph, from, player.stand, state).is_none())
}

/// Where a Stranded Zombie is in its trip to the door and its pounding.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Trip {
    /// Walking to the path node on its side of the door.
    ToNode,
    /// Walking the last stretch, straight at the door.
    ToDoor,
    /// Pounding.
    Pounding,
}

/// A Zombie left with no route to any Player by a seal, on its way to a Quarantine Door to pound on it.
#[derive(Component, Debug, Clone, Copy)]
pub struct Stranded {
    /// The path node on its side of the door.
    node: Vec3,
    /// The door's plane, at the floor.
    door: Vec3,
    trip: Trip,
    /// Seconds spent walking.
    walked: f32,
    /// Seconds spent pounding.
    pounded: f32,
    /// Seconds until the next swing, between blows.
    rest: f32,
    /// A swing is under way, whose landing is to sound.
    swinging: bool,
}

impl Stranded {
    fn new(node: Vec3, door: Vec3) -> Self {
        Self { node, door, trip: Trip::ToNode, walked: 0.0, pounded: 0.0, rest: 0.0, swinging: false }
    }

    /// Whether it is at the door, pounding.
    #[cfg(test)]
    pub fn pounding(&self) -> bool {
        self.trip == Trip::Pounding
    }
}

/// How long a Stranded Zombie waits between one blow on the door and the next.
const POUND_REST_SECS: f32 = 0.35;
/// How close (flat) to its path node a Stranded Zombie must be to turn for the door.
const NODE_REACHED: f32 = 1.0;

/// A special or the Sapper cut off by a seal, to be moved to a fresh Special Spawn Point in the
/// physics step, as the stuck rule moves it.
#[derive(Component, Debug)]
pub struct Relocate {
    to: Vec3,
}

/// The Quarantine Doors of the Map, each with its doorway and its Wing.
#[derive(Default)]
pub struct Doorways {
    map: Option<MapId>,
    list: Vec<(PieceId, crate::maps::quarantine::WingId, Doorway)>,
}

impl Doorways {
    fn of(&mut self, graph: &NavGraph) -> &[(PieceId, crate::maps::quarantine::WingId, Doorway)] {
        if self.map != Some(graph.map) {
            let layout = graph.map.layout();
            self.list = pieces(layout)
                .filter_map(|(id, piece)| Some((id, piece.wing()?, Doorway::of(graph, id, piece)?)))
                .collect();
            self.map = Some(graph.map);
        }
        &self.list
    }
}

/// The nearest shut Quarantine Door (of the Wing that is out) that a Zombie at `from` can walk to on
/// its side: the path node it will go to, and the door's plane at the floor. `None` if it can walk to
/// neither side of any.
pub fn nearest_shut_door(
    graph: &NavGraph,
    doorways: &[(PieceId, crate::maps::quarantine::WingId, Doorway)],
    wing: Option<crate::maps::quarantine::WingId>,
    from: Vec3,
    state: NavState,
) -> Option<(Vec3, Vec3)> {
    use crate::enemies::target::path_distance;
    doorways
        .iter()
        .filter(|(_, door_wing, _)| Some(*door_wing) == wing)
        .flat_map(|(_, _, doorway)| [(doorway.landing, doorway.centre), (doorway.inside, doorway.centre)])
        .filter_map(|(node, door)| Some((path_distance(graph, from, node, state)?, node, door)))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, node, door)| (node, door))
}

/// Looks for Zombies the seal has cut off, Strands the ones that have been for `QUARANTINE_STRAND_SECS`,
/// and takes the Strand off one that can reach a Player again or when no Wing is Sealed or Sealing.
pub(super) fn strand_zombies(
    mut commands: Commands,
    mut zombies: Query<
        (Entity, &mut Zombie, &Transform, Option<&mut CutOff>, Option<&Stranded>, Has<CarriesOver>, Has<Surge>),
        (Without<Dying>, Without<Rising>, Without<Entering>, Without<Telegraph>, Without<Requested>, Without<Relocate>, Without<Player>),
    >,
    spawn_points: Query<(&Transform, &SpawnPoint), (Without<Zombie>, Without<Player>)>,
    players: Players,
    graph: Res<NavGraph>,
    doors: Query<&Door>,
    pieces_q: Query<&PieceState>,
    barriers: Res<Barriers>,
    quarantine: Option<Res<Quarantine>>,
    active: Option<Res<ActiveZones>>,
    mut doorways: Local<Doorways>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    let sealing = quarantine.as_ref().is_some_and(|quarantine| quarantine.stage().seals());
    let wing = quarantine.as_ref().and_then(|quarantine| quarantine.wing());
    let layout = graph.map.layout();
    if layout.wings.is_empty() {
        return;
    }
    let open_doors: Vec<_> = doors.iter().filter(|door| door.open).filter_map(|door| door.route()).collect();
    let rests = standing_rests(pieces_q.iter().map(|state| (state.id(), state.phase())), layout.shifting.len());
    let barrier_open = |b| barriers.is_open(b);
    let state = NavState::new(&open_doors, &barrier_open).with_rests(&rests);
    let anchors: Vec<(ZoneId, Vec3)> = players.spawn_anchors().iter().map(|spot| (spot.zone, spot.centre)).collect();

    for (entity, mut zombie, transform, watch, stranded, carries_over, surge) in &mut zombies {
        // What carries over between Rounds is its own module's to move, but for the specials.
        let special = zombie.special_kind().is_some();
        if (carries_over && !surge && !special) || zombie.hellhound && carries_over {
            continue;
        }
        let Some(mut watch) = watch else {
            commands.entity(entity).insert(CutOff::default());
            continue;
        };
        if !sealing {
            watch.reset();
            if stranded.is_some() {
                release(&mut commands, entity, &mut zombie);
            }
            continue;
        }
        let half = zombie.half_height();
        let feet = transform.translation - Vec3::Y * half;
        watch.check_in -= dt;
        let reachable = if watch.check_in <= 0.0 {
            watch.check_in = QUARANTINE_STRAND_CHECK_SECS;
            !cut_off(&graph, feet, players.iter(), state)
        } else {
            watch.reachable
        };
        let is_stranded = watch.advance(reachable, sealing, dt);
        if reachable {
            if stranded.is_some() {
                release(&mut commands, entity, &mut zombie);
            }
            continue;
        }
        if !is_stranded || stranded.is_some() {
            continue;
        }
        if special {
            // A special keeps its place in the Horde: it takes the stuck rule's way out, to a fresh
            // Special Spawn Point.
            let points = special_spawn_candidates(
                spawn_points.iter().filter(|(_, point)| point.kind == SpawnKind::Special).map(|(t, point)| (point.zone, t.translation)),
                &anchors,
                active.as_ref().map_or_else(Default::default, |active| active.0),
            );
            if !points.is_empty() {
                let at = points[rand::random_range(0..points.len())];
                commands.entity(entity).insert(Relocate { to: at + Vec3::Y * (half + 0.02) });
            }
            watch.reset();
            continue;
        }
        match nearest_shut_door(&graph, doorways.of(&graph), wing, feet, state) {
            Some((node, door)) => {
                commands.entity(entity).insert((Stranded::new(node, door), Seeking(node)));
            }
            // Nowhere to pound: straight back into the Horde.
            None => go_back(&mut commands, entity, surge),
        }
    }
}

/// Takes the Strand off a Zombie: it chases again.
fn release(commands: &mut Commands, entity: Entity, zombie: &mut Zombie) {
    zombie.wind_up = None;
    commands.entity(entity).remove::<(Stranded, Seeking)>();
}

/// A Stranded Zombie goes back into the Horde: quietly removed, as the stuck rule removes one. A Surge
/// Zombie goes back into no Horde.
fn go_back(commands: &mut Commands, entity: Entity, surge: bool) {
    commands.entity(entity).despawn();
    if !surge {
        commands.queue(|world: &mut World| {
            world.write_message(ZombieCleanedUp);
        });
    }
}

/// Walks each Stranded Zombie to its door, and has it pound on it, and sends it back into the Horde
/// when it is done. After `attack`, so the swing it sets is the one `attack` runs down.
pub(super) fn pound_doors(
    mut commands: Commands,
    mut zombies: Query<
        (Entity, &mut Zombie, &mut Stranded, &Transform, Has<Surge>),
        (Without<Dying>, Without<Stunned>, Without<KnockedDown>, Without<Player>),
    >,
    players: Players,
    mut cues: MessageWriter<PlayCue>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    // With no Player up there is nobody to be cut off from: the Strand is lifted.
    let anyone_up = players.up_count() > 0;
    for (entity, mut zombie, mut stranded, transform, surge) in &mut zombies {
        if !anyone_up {
            release(&mut commands, entity, &mut zombie);
            continue;
        }
        let feet = transform.translation - Vec3::Y * zombie.half_height();
        let flat = |to: Vec3| (to - feet).with_y(0.0).length();
        match stranded.trip {
            Trip::ToNode | Trip::ToDoor => {
                stranded.walked += dt;
                if stranded.walked >= QUARANTINE_STRAND_WALK_SECS {
                    go_back(&mut commands, entity, surge);
                } else if stranded.trip == Trip::ToNode && flat(stranded.node) <= NODE_REACHED {
                    stranded.trip = Trip::ToDoor;
                    commands.entity(entity).insert(Seeking(stranded.door));
                } else if stranded.trip == Trip::ToDoor && flat(stranded.door) <= QUARANTINE_POUND_REACH {
                    stranded.trip = Trip::Pounding;
                }
            }
            Trip::Pounding => {
                stranded.pounded += dt;
                if stranded.pounded >= QUARANTINE_POUND_SECS {
                    go_back(&mut commands, entity, surge);
                    continue;
                }
                if zombie.wind_up.is_none() {
                    // The blow came down: it sounds on the door, and the next follows a beat later.
                    if std::mem::take(&mut stranded.swinging) {
                        cues.write(PlayCue::new(Cue::DoorPound, Some(stranded.door + Vec3::Y * 1.2)));
                        stranded.rest = POUND_REST_SECS;
                    }
                    stranded.rest -= dt;
                    if stranded.rest <= 0.0 {
                        zombie.wind_up = Some(zombie.attack_stats().1);
                        stranded.swinging = true;
                    }
                }
            }
        }
    }
}

/// Moves a special or the Sapper a seal has cut off to its fresh Special Spawn Point, in the physics
/// step as a clean cut: the easing starts from the new spot too, and its route is planned again.
pub(super) fn relocate_stranded(
    mut commands: Commands,
    mut moved: Query<(Entity, &Relocate, &mut Transform, &mut TranslationEasingState, &mut Route), Without<Dying>>,
) {
    for (entity, relocate, mut transform, mut easing, mut route) in &mut moved {
        commands.entity(entity).remove::<Relocate>();
        transform.translation = relocate.to;
        easing.start = Some(transform.translation);
        *route = Route::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spot(index: u32, feet: Vec3, up: bool) -> PlayerSpot {
        PlayerSpot {
            entity: Entity::from_raw_u32(index).unwrap(),
            centre: feet + Vec3::Y,
            feet,
            stand: feet,
            zone: ZoneId::StartRoom,
            up,
            local: false,
        }
    }

    /// Every Door bought, and the Quarantine Doors of the West Wing (pieces 0 and 1) Sealed.
    fn west_wing_sealed() -> (NavGraph, Vec<(ZoneId, ZoneId)>, Vec<Option<u8>>) {
        let graph = NavGraph::new(MapId::Sanatorium);
        let joins = ZoneId::ALL.iter().flat_map(|&a| ZoneId::ALL.iter().map(move |&b| (a, b))).collect();
        (graph, joins, vec![Some(1), Some(1), Some(0), Some(0), Some(0), Some(0)])
    }

    const OUTSIDE: Vec3 = Vec3::new(-30.0, 0.0, 11.0);
    const INSIDE: Vec3 = Vec3::new(-40.0, 0.0, 28.0);

    #[test]
    fn a_zombie_cut_off_for_five_seconds_while_a_wing_is_sealed_is_stranded() {
        let mut watch = CutOff::default();
        let mut stranded_at = None;
        for tick in 0..120 {
            if watch.advance(false, true, 0.1) && stranded_at.is_none() {
                stranded_at = Some(tick);
            }
        }
        // The fiftieth tenth of a second (or the one after, with float rounding): five seconds.
        assert!(matches!(stranded_at, Some(49 | 50)), "{stranded_at:?}");
        assert_eq!(QUARANTINE_STRAND_SECS, 5.0);
    }

    #[test]
    fn a_zombie_that_can_reach_a_player_is_never_stranded() {
        let mut watch = CutOff::default();
        assert!((0..100_000).all(|_| !watch.advance(true, true, 0.1)), "ten thousand seconds, sealed, with a route");
        // Nor one with no route while no Wing is Sealed or Sealing.
        let mut watch = CutOff::default();
        assert!((0..100_000).all(|_| !watch.advance(false, false, 0.1)), "ten thousand seconds with no seal");
        // A route found in time starts it over.
        let mut watch = CutOff::default();
        for _ in 0..40 {
            assert!(!watch.advance(false, true, 0.1));
        }
        assert!(!watch.advance(true, true, 0.1));
        assert!((0..40).all(|_| !watch.advance(false, true, 0.1)), "the clock starts over");
        // On the Map: a Player anyone can walk to, or any one of several, or none up at all.
        let (graph, joins, rests) = west_wing_sealed();
        let state = NavState::new(&joins, &|_| true).with_rests(&rests);
        let outside = spot(1, OUTSIDE + Vec3::X * 4.0, true);
        let inside = spot(2, INSIDE, true);
        assert!(!cut_off(&graph, OUTSIDE, [outside], state), "a Player outside is reachable from outside");
        assert!(!cut_off(&graph, OUTSIDE, [inside, outside], state), "reaching one of two is enough");
        assert!(!cut_off(&graph, OUTSIDE, [spot(3, INSIDE, false)], state), "with nobody up there is nobody to be cut off from");
        assert!(!cut_off(&graph, OUTSIDE, [], state));
    }

    #[test]
    fn a_zombie_outside_a_sealed_wing_is_cut_off_from_a_player_inside_it_and_not_once_it_is_open() {
        let (graph, joins, rests) = west_wing_sealed();
        let sealed = NavState::new(&joins, &|_| true).with_rests(&rests);
        let inside = spot(1, INSIDE, true);
        assert!(cut_off(&graph, OUTSIDE, [inside], sealed), "no way in");
        // A Zombie inside with the Player outside is cut off just the same.
        assert!(cut_off(&graph, INSIDE, [spot(2, OUTSIDE, true)], sealed));
        // Inside with the Player, it is not.
        assert!(!cut_off(&graph, INSIDE + Vec3::Z * 3.0, [inside], sealed));
        let open_rests = vec![Some(0); 6];
        let open = NavState::new(&joins, &|_| true).with_rests(&open_rests);
        assert!(!cut_off(&graph, OUTSIDE, [inside], open), "the doors open, and the way is back");
        // A Swinging door passes nobody.
        let swinging_rests = vec![None, None, Some(0), Some(0), Some(0), Some(0)];
        let swinging = NavState::new(&joins, &|_| true).with_rests(&swinging_rests);
        assert!(cut_off(&graph, OUTSIDE, [inside], swinging));
    }

    #[test]
    fn a_stranded_zombie_goes_to_the_nearest_shut_door_on_its_side() {
        let (graph, joins, rests) = west_wing_sealed();
        let state = NavState::new(&joins, &|_| true).with_rests(&rests);
        let mut doorways = Doorways::default();
        let list = doorways.of(&graph).to_vec();
        assert_eq!(list.len(), 6);
        let wing = Some(crate::maps::quarantine::WingId(0));
        // From the West Corridor, the Corridor's door Q1; from the Refectory, Q2.
        let (node, door) = nearest_shut_door(&graph, &list, wing, Vec3::new(-36.0, 0.0, 11.0), state).unwrap();
        assert!(node.distance(Vec3::new(-40.0, 0.0, 11.5)) < 0.1, "{node}");
        assert!((door.x + 40.0).abs() < 0.1 && (door.z - 13.77).abs() < 0.1, "{door}");
        let (node, door) = nearest_shut_door(&graph, &list, wing, Vec3::new(-20.0, 0.0, 49.0), state).unwrap();
        assert!(node.distance(Vec3::new(-23.5, 0.0, 49.0)) < 0.1, "{node}");
        assert!((door.x + 25.77).abs() < 0.1, "{door}");
        // Inside the Wing, a door's inside: the Long Ward's.
        let (node, _) = nearest_shut_door(&graph, &list, wing, Vec3::new(-40.0, 0.0, 22.0), state).unwrap();
        assert!(node.distance(Vec3::new(-40.0, 0.0, 16.5)) < 0.1, "{node}");
        // No door of another Wing counts.
        assert!(nearest_shut_door(&graph, &list, None, Vec3::new(-36.0, 0.0, 11.0), state).is_none());
    }
}
