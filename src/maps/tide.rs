//! The Tide: one clock for a Map's Canals. It moves a phase each Intermission, Low, Rising, High,
//! Falling and round again, through three water levels (Rising and Falling share Mid). The water
//! moves in a Turn, after bells; its rules (`water_at`, the Deep Water walls, nav, Wading Points)
//! change once, when the Tide lands. Only the Polder has Canals; on any other Map the Tide stays
//! Low and never turns.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::audio::PlayCue;
use crate::audio::cues::Cue;
use crate::authority::HostSimulation;
use crate::maps::map::{Layout, MapId};
use crate::maps::water::{Canal, CanalId, Level, TideLevel};
use crate::maps::zone::ZoneId;
use crate::run::round::{Round, RoundEnded, RoundStarted};
use crate::state::{GameState, input_runs, world_runs};
use crate::tuning::*;

/// Where the Tide is in its cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash, Serialize, Deserialize)]
pub enum TidePhase {
    #[default]
    Low,
    Rising,
    High,
    Falling,
}

impl TidePhase {
    /// The phase Round `n` is played at: Round 1 is Low, 2 Rising, 3 High, 4 Falling, 5 Low again.
    pub fn of_round(n: u32) -> Self {
        match n.saturating_sub(1) % 4 {
            0 => TidePhase::Low,
            1 => TidePhase::Rising,
            2 => TidePhase::High,
            _ => TidePhase::Falling,
        }
    }

    /// The water level it stands at.
    pub fn level(self) -> TideLevel {
        match self {
            TidePhase::Low => TideLevel::Low,
            TidePhase::Rising | TidePhase::Falling => TideLevel::Mid,
            TidePhase::High => TideLevel::High,
        }
    }

    /// The phase after it.
    pub fn next(self) -> Self {
        match self {
            TidePhase::Low => TidePhase::Rising,
            TidePhase::Rising => TidePhase::High,
            TidePhase::High => TidePhase::Falling,
            TidePhase::Falling => TidePhase::Low,
        }
    }
}

/// The four phases, in the order the Tide turns through them.
pub const TIDE_PHASES: [TidePhase; 4] = [TidePhase::Low, TidePhase::Rising, TidePhase::High, TidePhase::Falling];

/// A set of Tide levels, for a path edge that can only be walked at some of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TideMask(pub u8);

impl TideMask {
    pub const LOW: TideMask = TideMask(1);
    pub const MID: TideMask = TideMask(2);
    pub const HIGH: TideMask = TideMask(4);
    pub const ALL: TideMask = TideMask(7);

    pub const fn or(self, other: TideMask) -> TideMask {
        TideMask(self.0 | other.0)
    }

    pub fn contains(self, level: TideLevel) -> bool {
        self.0 & (1 << level.index()) != 0
    }

    /// The levels in it.
    pub fn levels(self) -> impl Iterator<Item = TideLevel> {
        TideLevel::ALL.into_iter().filter(move |&level| self.contains(level))
    }
}

/// What the Tide is doing between phases.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum TurnState {
    #[default]
    Still,
    /// The bells toll, `elapsed` seconds in.
    Bells { elapsed: f32 },
    /// The water moves to the next phase's level, `elapsed` seconds in.
    Turning { elapsed: f32 },
}

impl TurnState {
    /// The same state of the Turn, whatever the seconds.
    pub fn kind(self) -> u8 {
        match self {
            TurnState::Still => 0,
            TurnState::Bells { .. } => 1,
            TurnState::Turning { .. } => 2,
        }
    }
}

/// Smoothstep over 0..1.
pub fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The Tide. Its `phase` is the one the rules stand at; during a Turn the water is drawn on its way
/// to the next.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct Tide {
    pub phase: TidePhase,
    pub turn: TurnState,
}

impl Tide {
    /// The water level the rules stand at.
    pub fn level(&self) -> TideLevel {
        self.phase.level()
    }

    /// How high `canal`'s surface is drawn: its level, or on the way to the next over a Turn.
    pub fn drawn(&self, canal: &Canal) -> f32 {
        let from = canal.at(self.phase.level());
        match self.turn {
            TurnState::Turning { elapsed } => {
                let to = canal.at(self.phase.next().level());
                from + (to - from) * ease(elapsed / TIDE_TURN_SECS)
            }
            _ => from,
        }
    }

    /// How strongly the foam of `level` shows: 1 at its level, fading out as the Turn leaves it and
    /// in as the Turn arrives.
    pub fn foam_weight(&self, level: TideLevel) -> f32 {
        let now = self.phase.level();
        match self.turn {
            TurnState::Turning { elapsed } => {
                let e = ease(elapsed / TIDE_TURN_SECS);
                let to = self.phase.next().level();
                (if level == now { 1.0 - e } else { 0.0 }) + (if level == to { e } else { 0.0 })
            }
            _ => f32::from(level == now),
        }
    }

    /// Whether a Turn is under way.
    pub fn turning(&self) -> bool {
        self.turn != TurnState::Still
    }
}

/// The Tide has landed at a new phase: Washed Up follows.
#[derive(Message, Debug, Clone, Copy)]
pub struct TideLanded {
    pub level: TideLevel,
}

/// Lands the Tide at `phase`.
fn land(tide: &mut Tide, phase: TidePhase, landed: &mut MessageWriter<TideLanded>) {
    tide.phase = phase;
    tide.turn = TurnState::Still;
    landed.write(TideLanded { level: phase.level() });
}

/// Where the Pumping Station's bell tolls: its centre, if the Map has one.
pub fn bell_at(layout: &Layout) -> Option<Vec3> {
    layout.zones.contains(&ZoneId::PumpingStation).then(|| {
        let (lo, hi) = ZoneId::PumpingStation.volume();
        (lo + hi) / 2.0
    })
}

/// The middle of a Canal's pools, at `y`.
pub fn canal_centre(layout: &Layout, id: CanalId, y: f32) -> Vec3 {
    let (mut lo, mut hi) = (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
    for pool in layout.water.iter().filter(|pool| pool.level == Level::Canal(id)) {
        lo = lo.min(pool.min);
        hi = hi.max(pool.max);
    }
    let middle = (lo + hi) / 2.0;
    Vec3::new(middle.x, y, middle.y)
}

/// Back to Low and Still as a Run begins.
fn reset_tide(mut tide: ResMut<Tide>) {
    *tide = Tide::default();
}

/// Runs the Turn's clock on every machine; a client's stops at the end of what it is told until
/// the Host says the next.
fn advance_turn(mut tide: ResMut<Tide>, time: Res<Time>) {
    let dt = time.delta_secs();
    match tide.turn {
        TurnState::Still => {}
        TurnState::Bells { elapsed } => tide.turn = TurnState::Bells { elapsed: (elapsed + dt).min(TIDE_BELL_SECS) },
        TurnState::Turning { elapsed } => tide.turn = TurnState::Turning { elapsed: (elapsed + dt).min(TIDE_TURN_SECS) },
    }
}

/// The Host moves the Turn on: the bells give way to the water, and the water to the landing.
fn step_turn(mut tide: ResMut<Tide>, mut landed: MessageWriter<TideLanded>) {
    match tide.turn {
        TurnState::Bells { elapsed } if elapsed >= TIDE_BELL_SECS => tide.turn = TurnState::Turning { elapsed: 0.0 },
        TurnState::Turning { elapsed } if elapsed >= TIDE_TURN_SECS => {
            let next = tide.phase.next();
            land(&mut tide, next, &mut landed);
        }
        _ => {}
    }
}

/// A Round's end starts the Turn with the bells, on a Map with Canals.
fn begin_turn(mut ended: MessageReader<RoundEnded>, mut tide: ResMut<Tide>, map: Option<Res<MapId>>) {
    if ended.read().count() == 0 || tide.turning() || map.is_none_or(|map| map.layout().canals.is_empty()) {
        return;
    }
    tide.turn = TurnState::Bells { elapsed: 0.0 };
}

/// The next Round starting finishes any Turn at once, and puts the Tide at the Round's phase.
fn settle_on_round_start(
    mut started: MessageReader<RoundStarted>,
    round: Option<Res<Round>>,
    mut tide: ResMut<Tide>,
    map: Option<Res<MapId>>,
    mut landed: MessageWriter<TideLanded>,
) {
    if started.read().count() == 0 || map.is_none_or(|map| map.layout().canals.is_empty()) {
        return;
    }
    let Some(round) = round else { return };
    let phase = TidePhase::of_round(round.number);
    if tide.turning() || tide.phase != phase {
        land(&mut tide, phase, &mut landed);
    }
}

/// The debug key turns the Tide to its next phase now, with no bells.
fn turn_now(mut tide: ResMut<Tide>, map: Option<Res<MapId>>) {
    if map.is_some_and(|map| !map.layout().canals.is_empty()) && !tide.turning() {
        tide.turn = TurnState::Turning { elapsed: 0.0 };
    }
}

/// Tolls the bell once a second through the bells, and sets the rush of the water going from each
/// Canal as the Turn begins, on whatever machine hears them.
fn tide_cues(
    tide: Res<Tide>,
    map: Option<Res<MapId>>,
    mut last: Local<(u8, i32)>,
    mut cues: MessageWriter<PlayCue>,
) {
    let Some(layout) = map.map(|map| map.layout()).filter(|layout| !layout.canals.is_empty()) else { return };
    let kind = tide.turn.kind();
    if kind != last.0 {
        *last = (kind, -1);
        if kind == 2 {
            for (index, canal) in layout.canals.iter().enumerate() {
                let at = canal_centre(layout, CanalId(index as u8), canal.at(tide.level()));
                cues.write(PlayCue::new(Cue::TideRush, Some(at)));
            }
        }
    }
    if let TurnState::Bells { elapsed } = tide.turn {
        let toll = elapsed.floor() as i32;
        if toll > last.1 && toll < TIDE_BELL_SECS as i32 {
            last.1 = toll;
            cues.write(PlayCue::new(Cue::TideBell, bell_at(layout)));
        }
    }
}

/// The white-blue splash and the gasp when the Player is Washed Up: the local Player moved by more
/// than `WASHED_UP_MIN_MOVE` in a frame while the Tide lands (or just after), as the Host moves
/// them, or a client sees the Host's correction. Inferred on every machine, so no message is needed.
fn washed_up_feedback(
    tide: Res<Tide>,
    player: Query<&Transform, With<crate::players::player::LocalPlayer>>,
    mut last: Local<(Option<Vec3>, TidePhase, f32)>,
    mut overlays: Option<ResMut<crate::ui::hud::overlay::Overlays>>,
    mut cues: MessageWriter<PlayCue>,
    time: Res<Time>,
) {
    if tide.phase != last.1 {
        last.1 = tide.phase;
        last.2 = 1.0;
    }
    last.2 = (last.2 - time.delta_secs()).max(0.0);
    let Ok(transform) = player.single() else { return };
    let now = transform.translation;
    if let Some(was) = last.0
        && last.2 > 0.0
        && was.distance(now) > WASHED_UP_MIN_MOVE
    {
        last.2 = 0.0;
        cues.write(PlayCue::new(Cue::WashedUp, None));
        if let Some(overlays) = overlays.as_mut() {
            overlays.pulse(crate::ui::hud::overlay::Overlay::WashedUp, 1.0, WASHED_UP_SPLASH_SECS);
        }
    }
    last.0 = Some(now);
}

/// Switches the path graph to the Tide's level, and has every Zombie plan again.
fn sync_nav(tide: Res<Tide>, graph: Option<ResMut<crate::enemies::nav::NavGraph>>, mut routes: Query<&mut crate::enemies::nav::Route>) {
    let Some(mut graph) = graph else { return };
    if graph.tide != tide.level() {
        graph.tide = tide.level();
        for mut route in &mut routes {
            route.repath_in = 0.0;
        }
    }
}

/// The systems that turn and land the Tide, on the Host.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TideTurn;

pub struct TidePlugin;

impl Plugin for TidePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Tide>()
            .add_message::<TideLanded>()
            .add_message::<RoundEnded>()
            .add_message::<RoundStarted>()
            .add_message::<PlayCue>()
            .add_systems(OnEnter(GameState::Playing), reset_tide)
            .add_systems(Update, (advance_turn, (tide_cues, sync_nav, washed_up_feedback)).chain().run_if(world_runs))
            .add_systems(
                Update,
                (begin_turn, settle_on_round_start, step_turn.after(advance_turn))
                    .chain()
                    .in_set(TideTurn)
                    .in_set(HostSimulation)
                    .run_if(world_runs),
            )
            .add_systems(
                Update,
                turn_now
                    .in_set(HostSimulation)
                    .run_if(world_runs)
                    .run_if(input_runs)
                    .run_if(|keys: Option<Res<ButtonInput<KeyCode>>>| keys.is_some_and(|keys| keys.just_pressed(KeyCode::Backslash))),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maps::map::{Layout, MapId};

    #[test]
    fn rounds_play_at_the_phase_of_their_number_and_phases_have_three_levels() {
        let phases: Vec<_> = (1..=9).map(TidePhase::of_round).collect();
        use TidePhase::*;
        assert_eq!(phases, [Low, Rising, High, Falling, Low, Rising, High, Falling, Low]);
        assert_eq!(TidePhase::of_round(0), Low);
        let levels: Vec<_> = [Low, Rising, High, Falling].map(TidePhase::level).into();
        assert_eq!(levels, [TideLevel::Low, TideLevel::Mid, TideLevel::High, TideLevel::Mid]);
        for phase in [Low, Rising, High, Falling] {
            assert_eq!(phase.next().next().next().next(), phase);
        }
    }

    #[test]
    fn the_bells_and_the_turn_fit_in_the_intermission() {
        assert!(TIDE_BELL_SECS + TIDE_TURN_SECS < INTERMISSION_SECS);
    }

    #[test]
    fn the_drawn_ease_ends_on_each_level() {
        let canal = Canal { name: "T", low: -1.7, mid: -1.1, high: -0.4 };
        for phase in [TidePhase::Low, TidePhase::Rising, TidePhase::High, TidePhase::Falling] {
            let still = Tide { phase, turn: TurnState::Still };
            assert_eq!(still.drawn(&canal), canal.at(phase.level()));
            let start = Tide { phase, turn: TurnState::Turning { elapsed: 0.0 } };
            let end = Tide { phase, turn: TurnState::Turning { elapsed: TIDE_TURN_SECS } };
            assert_eq!(start.drawn(&canal), canal.at(phase.level()));
            assert!((end.drawn(&canal) - canal.at(phase.next().level())).abs() < 1e-6);
            let half = Tide { phase, turn: TurnState::Turning { elapsed: TIDE_TURN_SECS / 2.0 } };
            let mid = (canal.at(phase.level()) + canal.at(phase.next().level())) / 2.0;
            assert!((half.drawn(&canal) - mid).abs() < 1e-6);
            assert_eq!(start.foam_weight(phase.level()), 1.0);
            assert_eq!(end.foam_weight(phase.next().level()), 1.0);
            assert!((half.foam_weight(phase.level()) + half.foam_weight(phase.next().level()) - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn a_mask_holds_its_levels() {
        let m = TideMask::LOW.or(TideMask::MID);
        assert!(m.contains(TideLevel::Low) && m.contains(TideLevel::Mid) && !m.contains(TideLevel::High));
        assert_eq!(TideMask::ALL.levels().count(), 3);
    }

    // ---- Per Map, at every level ----

    use crate::enemies::nav::{Gate, NavGraph};
    use crate::maps::map::SpawnKind;
    use crate::maps::water::{Wading, water_at, wading_levels};
    use crate::run::round::horde_points;

    fn nodes_of(layout: &Layout) -> std::collections::HashMap<&'static str, Vec3> {
        layout.nav_nodes.iter().map(|&(name, x, y, z)| (name, Vec3::new(x, y, z))).collect()
    }

    /// Every `Gate::Tide` edge is clear of Deep Water at each level in its mask, every other edge
    /// at all three, and no node is Deep at a level where an edge that is open then touches it.
    #[test]
    fn every_edge_is_clear_of_deep_water_at_every_level_it_is_open() {
        for map in MapId::ALL {
            let layout = map.layout();
            let nodes = nodes_of(layout);
            for &(a, b, gate) in layout.nav_edges {
                let levels: Vec<TideLevel> = match gate {
                    Gate::Tide(mask) => mask.levels().collect(),
                    _ => TideLevel::ALL.to_vec(),
                };
                for level in levels {
                    for k in 0..=40 {
                        let at = nodes[a].lerp(nodes[b], k as f32 / 40.0);
                        assert_ne!(water_at(layout, level, at), Some(Wading::Deep), "{a} - {b} on the {} crosses Deep Water at {level:?}, {at}", map.name());
                    }
                }
            }
        }
    }

    /// Nothing the Map places stands in Deep Water at any level: Seats, Buyables, Box Locations,
    /// Levers, and the Spawn Points that aren't Wading Points.
    #[test]
    fn nothing_a_map_places_is_ever_in_deep_water() {
        for map in MapId::ALL {
            let layout = map.layout();
            for level in TideLevel::ALL {
                let deep = |at: Vec3| water_at(layout, level, at) == Some(Wading::Deep);
                for (name, at, _) in crate::maps::map::tests::fixtures(map) {
                    assert!(!deep(at), "{name} on the {} is in Deep Water at {level:?}", map.name());
                }
                for seat in layout.seats {
                    assert!(!deep(seat), "a Seat on the {} is in Deep Water at {level:?}", map.name());
                }
                for &(zone, at) in layout.risers.iter().chain(layout.hellhound_spawns).chain(layout.special_spawns) {
                    assert!(!deep(at), "a Spawn Point in {} on the {} is in Deep Water at {level:?}", zone.name(), map.name());
                }
            }
        }
    }

    /// Pools that touch have one surface at every level: the same height, or Canals that are alike.
    #[test]
    fn pools_that_touch_share_a_canal_or_one_surface() {
        for map in MapId::ALL {
            let layout = map.layout();
            for (i, a) in layout.water.iter().enumerate() {
                for b in &layout.water[i + 1..] {
                    let touch = a.min.x <= b.max.x && b.min.x <= a.max.x && a.min.y <= b.max.y && b.min.y <= a.max.y;
                    if touch && a.bed == b.bed {
                        for level in TideLevel::ALL {
                            assert_eq!(a.surface(layout.canals, level), b.surface(layout.canals, level), "{} and {} touch at {level:?}", a.zone.name(), b.zone.name());
                        }
                    }
                }
            }
        }
    }

    /// The Polder's twelve Wading Points: a channel one at Low and two berm ones at Mid for each
    /// Ford, and none on a far berm.
    #[test]
    fn the_polders_wading_points_are_active_at_their_levels_and_never_on_a_far_berm() {
        let layout = MapId::Polder.layout();
        assert_eq!(layout.wading.len(), 12);
        let (mut channel, mut berm) = (0, 0);
        for &(zone, at) in layout.wading {
            let table = wading_levels(layout, at);
            assert!(table.iter().any(|&on| on), "the Wading Point at {at} in {} is never active", zone.name());
            if at.y < -2.0 {
                channel += 1;
                assert_eq!(table, [true, false, false], "channel point at {at}");
            } else {
                berm += 1;
                assert_eq!(table, [false, true, false], "berm point at {at}");
                // On its own Zone's side of the Canal, not the far one.
                let own_south = matches!(zone, ZoneId::WeighHouse | ZoneId::CheeseMarket);
                assert!(if own_south { at.z < -6.0 } else { at.z > 30.0 }, "the berm point at {at} is on a far berm");
            }
        }
        assert_eq!((channel, berm), (4, 8));
    }

    /// At every level, every Zone with a Wading Point still has somewhere to spawn from: a window
    /// or one of its Wading Points that is active then.
    #[test]
    fn no_zone_ever_goes_silent() {
        for map in [MapId::Cove, MapId::Polder] {
            let layout = map.layout();
            for &(zone, _) in layout.wading {
                for level in TideLevel::ALL {
                    let windows = map.barriers().filter(|&barrier| crate::maps::map::WINDOWS[barrier.0 as usize].0 == zone).count();
                    let points = layout.wading.iter().filter(|&&(z, at)| z == zone && wading_levels(layout, at)[level.index()]).count();
                    assert!(windows + points > 0, "{} has nowhere to spawn from at {level:?}", zone.name());
                }
            }
        }
    }

    /// The round spawn pass takes channel Wading Points only at Low and berm ones only at Mid.
    #[test]
    fn the_spawn_pass_uses_channel_points_at_low_and_berm_points_at_mid() {
        let layout = MapId::Polder.layout();
        let spots: Vec<(ZoneId, Transform, SpawnKind)> =
            layout.wading.iter().map(|&(zone, at)| (zone, Transform::from_translation(at), SpawnKind::Wading)).collect();
        let every_zone: crate::maps::zone::ZoneSet = ZoneId::ALL.into_iter().collect();
        let picked = |level| {
            let points = horde_points(spots.iter().map(|(zone, t, kind)| (*zone, t, *kind)), Some(layout), level, every_zone, &[]);
            points.into_iter().map(|(t, _)| t.translation.y).collect::<Vec<_>>()
        };
        let low = picked(TideLevel::Low);
        assert_eq!(low.len(), 4);
        assert!(low.iter().all(|&y| y < -2.0));
        let mid = picked(TideLevel::Mid);
        assert_eq!(mid.len(), 8);
        assert!(mid.iter().all(|&y| y > -2.0));
        assert!(picked(TideLevel::High).is_empty());
    }

    /// A Zombie routes along a berm at Mid and not at High.
    #[test]
    fn a_zombie_routes_along_a_berm_at_mid_and_not_at_high() {
        let mut graph = NavGraph::new(MapId::Polder);
        let (west, east) = (graph.node("f43_bow").unwrap(), graph.node("f43_boe").unwrap());
        let route = |graph: &NavGraph| graph.plan(west, east, crate::enemies::nav::NavState::new(&[], &|_| true));
        graph.tide = TideLevel::Mid;
        assert!(route(&graph).is_some(), "along the berm at Mid");
        graph.tide = TideLevel::Low;
        assert!(route(&graph).is_some(), "the berm is dry at Low");
        graph.tide = TideLevel::High;
        assert!(route(&graph).is_none(), "the berm is Deep at High");
    }

    /// A water stair into another Zone has a Door at its head: from each Ford's stair top, the
    /// Zones a walk over Open and Tide edges reaches are the same as over Open edges alone, so the
    /// Tide joins no Zone to another.
    #[test]
    fn the_tide_never_opens_a_zone() {
        let graph = NavGraph::new(MapId::Polder);
        let reach = |start: usize, tide_too: bool| {
            let mut seen = vec![start];
            let mut frontier = vec![start];
            while let Some(here) = frontier.pop() {
                for edge in graph.edges.iter().filter(|edge| matches!(edge.gate, Gate::Open) || (tide_too && matches!(edge.gate, Gate::Tide(_)))) {
                    let there = if edge.from == here { edge.to } else if edge.to == here { edge.from } else { continue };
                    if !seen.contains(&there) {
                        seen.push(there);
                        frontier.push(there);
                    }
                }
            }
            let mut zones: Vec<_> = seen.iter().filter_map(|&i| graph.nodes[i].zone).collect();
            zones.sort_by_key(|zone| *zone as u16);
            zones.dedup();
            (zones, seen.len())
        };
        for name in ["f43_ot", "f26_ot", "f48_ot", "f47_ot"] {
            let start = graph.node(name).unwrap_or_else(|| panic!("no node {name}"));
            let (without, few) = reach(start, false);
            let (with, many) = reach(start, true);
            assert_eq!(without, with, "the Tide joins Zones from {name}");
            assert!(many > few, "{name}'s Ford is reached over the Tide's edges");
        }
    }

    /// Wherever Deep Water catches a body, the Washed Up target is dry or Shallow and in a Zone.
    #[test]
    fn a_washed_up_target_is_never_in_deep_water() {
        use crate::players::out_of_bounds::washed_up_target;
        let layout = MapId::Polder.layout();
        let graph = NavGraph::new(MapId::Polder);
        for level in TideLevel::ALL {
            for &(zone, feet) in layout.wading {
                let feet = feet + Vec3::Y * 0.1;
                let to = washed_up_target(&graph, layout, level, feet, Some(zone), None).expect("somewhere to go");
                assert_ne!(water_at(layout, level, to), Some(Wading::Deep), "{to} at {level:?}");
            }
        }
        // The channel is Deep at Mid, and the Ford's own Zone is where a Player there goes.
        let in_the_channel = Vec3::new(-47.0, -2.2, -6.0);
        assert!(layout.out_of_bounds(TideLevel::Mid, in_the_channel));
        assert!(!layout.out_of_bounds(TideLevel::Low, in_the_channel));
        let to = washed_up_target(&graph, layout, TideLevel::Mid, in_the_channel, Some(ZoneId::WeighHouse), None).unwrap();
        assert_eq!(crate::enemies::nav::zone_at(MapId::Polder, to + Vec3::Y * 0.1), Some(ZoneId::WeighHouse));
    }

    // ---- The Turn, headless ----

    use crate::enemies::zombie::headless::Sim;
    use crate::run::round::{Round, RoundEnded, RoundStarted};

    fn polder_sim() -> Sim {
        Sim::on(MapId::Polder, 60.0, |app| {
            app.add_plugins(TidePlugin).insert_resource(Round::in_progress(1)).insert_resource(crate::maps::zone::ActiveZones(ZoneId::ALL.into_iter().collect()));
        })
    }

    fn tide_of(sim: &mut Sim) -> Tide {
        *sim.world_mut().resource::<Tide>()
    }

    /// A Round's end runs Bells, then Turning, then the landing, and the next Round is played at
    /// the next phase; the Turn finishes at once if the next Round starts first.
    #[test]
    fn a_rounds_end_runs_the_bells_then_the_turn_then_the_landing() {
        let mut sim = polder_sim();
        assert_eq!(tide_of(&mut sim), Tide::default());
        sim.send(RoundEnded);
        sim.run(3);
        assert!(matches!(tide_of(&mut sim).turn, TurnState::Bells { .. }));
        sim.run(sim.frames(TIDE_BELL_SECS) + 2);
        assert!(matches!(tide_of(&mut sim).turn, TurnState::Turning { .. }), "{:?}", tide_of(&mut sim));
        assert_eq!(tide_of(&mut sim).level(), TideLevel::Low, "the rules hold through the Turn");
        sim.run(sim.frames(TIDE_TURN_SECS) + 2);
        assert_eq!(tide_of(&mut sim), Tide { phase: TidePhase::Rising, turn: TurnState::Still });

        // The next Round starting mid-Turn finishes it at once, at the Round's phase.
        sim.world_mut().insert_resource(Round::in_progress(3));
        sim.send(RoundEnded);
        sim.run(3);
        assert!(sim.world_mut().resource::<Tide>().turning());
        sim.send(RoundStarted);
        sim.run(3);
        assert_eq!(tide_of(&mut sim), Tide { phase: TidePhase::High, turn: TurnState::Still });
    }

    /// The debug key's Turn has no bells; a Map with no Canals never turns.
    #[test]
    fn a_map_without_canals_never_turns() {
        let mut sim = Sim::on(MapId::Cove, 60.0, |app| {
            app.add_plugins(TidePlugin).insert_resource(Round::in_progress(1));
        });
        sim.send(RoundEnded);
        sim.run(10);
        assert_eq!(tide_of(&mut sim), Tide::default());
    }

    /// A Player wading F1's channel when Low turns to Rising is Washed Up to dry ground with no
    /// Down, keeping their Last Stand when Down; a Power-up there is washed up too.
    #[test]
    fn a_player_in_the_channel_is_washed_up_when_the_tide_lands() {
        use crate::players::last_stand::LastStand;
        use crate::players::out_of_bounds::OutOfBoundsPlugin;
        use crate::players::player::{Health, Player};
        for down in [false, true] {
            let mut sim = Sim::on(MapId::Polder, 60.0, |app| {
                app.add_plugins((TidePlugin, OutOfBoundsPlugin))
                    .insert_resource(Round::in_progress(1))
                    .insert_resource(crate::maps::zone::ActiveZones(ZoneId::ALL.into_iter().collect()));
            });
            let layout = MapId::Polder.layout();
            let half = crate::tuning::PLAYER_RADIUS + crate::tuning::PLAYER_CAPSULE_LENGTH / 2.0;
            let channel = Vec3::new(-47.0, -2.2, -6.0);
            let player = {
                let world = sim.world_mut();
                let player = world.query_filtered::<Entity, With<Player>>().single(world).unwrap();
                world.entity_mut(player).insert(Health { current: 100.0, max: 100.0, since_hit: 0.0 });
                if down {
                    world.entity_mut(player).insert(LastStand::shown(30.0, false));
                }
                world.get_mut::<Transform>(player).unwrap().translation = channel + Vec3::Y * half;
                player
            };
            sim.run(2);
            assert!(!layout.out_of_bounds(TideLevel::Low, channel));
            let health = sim.world_mut().get::<Health>(player).unwrap().current;
            sim.world_mut().insert_resource(Tide { phase: TidePhase::Low, turn: TurnState::Turning { elapsed: TIDE_TURN_SECS } });
            sim.run(3);
            let world = sim.world_mut();
            assert_eq!(world.resource::<Tide>().phase, TidePhase::Rising);
            let feet = world.get::<Transform>(player).unwrap().translation - Vec3::Y * half;
            assert!(!layout.out_of_bounds(TideLevel::Mid, feet), "left at {feet}");
            assert!(feet.distance(channel) > 1.0, "not moved from {channel}");
            assert_eq!(world.get::<Health>(player).unwrap().current, health, "no damage");
            assert_eq!(world.get::<LastStand>(player).is_some(), down, "Last Stand untouched");
        }
    }

    /// The same rules at the layer under: a body is caught by Deep Water, or in the air over it,
    /// and a Power-up lying in it is washed to dry ground.
    #[test]
    fn the_air_over_deep_water_catches_a_body_and_a_power_up_is_washed_up() {
        let layout = MapId::Polder.layout();
        let channel = Vec3::new(-47.0, -2.2, -6.0);
        assert!(crate::maps::water::swamped(layout, TideLevel::Mid, channel));
        assert!(crate::maps::water::swamped(layout, TideLevel::Mid, channel + Vec3::Y * 2.0), "over it, within reach");
        assert!(!crate::maps::water::swamped(layout, TideLevel::Mid, channel + Vec3::Y * 4.5), "high over it");
        assert!(!crate::maps::water::swamped(layout, TideLevel::Low, channel));
    }
}
