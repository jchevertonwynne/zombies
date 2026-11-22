//! `SpawnZombie`: a request, from anything layered on Classic, for a Zombie of a kind to enter
//! play outside the Horde (ADR 0007). Classic owns the Zombie it spawns: its health, its entry and
//! its kind's rules. Each kind's module spawns its own: the special module the Napalm, the Sapper
//! module the Sapper, the hellhound module the Hellhound.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::audio::cues::Cue;
use crate::enemies::hellhound::hellhound_spawn_candidates;
use crate::enemies::sapper::window_candidates;
use crate::enemies::special::SpecialCue;
use crate::enemies::target::Players;
use crate::authority::HostSimulation;
use crate::enemies::zombie::{Heavyweight, SpeedTier, ZombieAssetsParam, spawn_zombie};
use crate::run::round::{CarriesOver, Round};
use crate::state::world_runs;
use crate::maps::barrier::Entering;
use crate::maps::barrier::Barriers;
use crate::maps::map::{BarrierId, MapId, SpawnKind, SpawnPoint};
use crate::maps::zone::{ActiveZones, ZoneId};
use crate::run::round::zombie_health;

/// The kinds of Zombie a `SpawnZombie` can ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestedKind {
    Napalm,
    Sapper,
    /// A Hellhound: it comes in a lightning strike, never through a Barrier, and bites as hounds do.
    Hellhound,
    /// A normal Zombie: a Walker, at the point asked for or through a window. Always its usual size.
    Walker,
    /// A normal Zombie at a Jogger's pace.
    Jogger,
    /// A normal Zombie at a Sprinter's pace.
    Sprinter,
    /// A Shrieker: it comes up out of the ground at the point asked for, as the special slot's does.
    Shrieker,
    /// A Nova Crawler: it comes up out of the ground at the point asked for.
    NovaCrawler,
}

impl RequestedKind {
    /// The Speed Tier of a normal Zombie kind, `None` for any other.
    pub fn normal_tier(self) -> Option<SpeedTier> {
        match self {
            RequestedKind::Walker => Some(SpeedTier::Walker),
            RequestedKind::Jogger => Some(SpeedTier::Jogger),
            RequestedKind::Sprinter => Some(SpeedTier::Sprinter),
            _ => None,
        }
    }
}

/// Where a requested Zombie enters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpawnAt {
    /// Rising out of the ground here, as its kind does.
    Point(Vec3),
    /// Through a window in an Active Zone, the Player's own Zone's first, every Plank ripped off
    /// at once: the Sapper's entry. A Hellhound, which never uses Barriers, comes in at a
    /// Hellhound Spawn Point in an Active Zone instead, as the Round's own hounds do.
    BarrierInActiveZone,
}

/// A Napalm's trail: a fire pool, as the one it rises from, dropped at its feet every `every_secs`
/// and lasting `pool_secs`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trail {
    pub every_secs: f32,
    pub pool_secs: f32,
}

/// Per-kind options, each with its Classic default.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpawnProfile {
    /// Napalm: it winds up and blows itself up near a Player, and its death bursts. Default true.
    pub detonates: bool,
    /// Napalm: the fire pools it leaves behind it. Default none.
    pub trail: Option<Trail>,
    /// Sapper: the shares of its max health at which its helmet re-forms (to hold until it has
    /// lost `SAPPER_HELMET_FRACTION` again). Default none: the helmet comes off once and stays off.
    pub helmet_reforms_at: &'static [f32],
    /// Sapper: how many systems each Sabotage takes at once. Default 1.
    pub sabotages_at_once: u32,
}

impl Default for SpawnProfile {
    fn default() -> Self {
        Self { detonates: true, trail: None, helmet_reforms_at: &[], sabotages_at_once: 1 }
    }
}

/// Spawn a Zombie of `kind` now, outside the Horde. Its health is the Round's base Zombie health
/// × `health_factor`, and its body is `scale` times the usual size. With `holds_round` the Round
/// stays open until it dies; without, it carries over like the special slot's Zombies. Classic
/// marks it `Requested`, and puts `heavyweight` on it if given.
#[derive(Message, Debug, Clone, Copy)]
pub struct SpawnZombie {
    pub kind: RequestedKind,
    pub health_factor: f32,
    pub scale: f32,
    pub at: SpawnAt,
    pub holds_round: bool,
    pub profile: SpawnProfile,
    pub heavyweight: Option<Heavyweight>,
    /// Whatever the requester wants to find the Zombie again by: put on it as `Requested::tag`.
    /// Classic reads nothing into it. Default none.
    pub tag: Option<u32>,
    /// Whether the Zombie stays out of the Alive Cap's count, so it never keeps a Horde Zombie from
    /// spawning. Default false: it counts, as a Boss does.
    pub outside_cap: bool,
}

/// A Zombie that entered play from a `SpawnZombie`, not from the Horde or Classic's own schedules.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Requested {
    /// The request's `tag`.
    pub tag: Option<u32>,
    /// The request's `outside_cap`.
    pub outside_cap: bool,
}

impl Requested {
    /// What a Zombie spawned for `request` carries.
    pub fn of(request: &SpawnZombie) -> Self {
        Self { tag: request.tag, outside_cap: request.outside_cap }
    }
}

/// A requested Zombie's health in Round `round`: the base Round health × `factor`.
pub fn requested_health(round: u32, factor: f32) -> f32 {
    zombie_health(round) as f32 * factor
}

/// Finds where a requested Zombie enters, and for a window, smashes its Planks off.
#[derive(SystemParam)]
pub struct Entry<'w, 's> {
    spawn_points: Query<'w, 's, (&'static Transform, &'static SpawnPoint)>,
    players: Players<'w, 's>,
    active: Res<'w, ActiveZones>,
    barriers: ResMut<'w, Barriers>,
    map: Option<Res<'w, MapId>>,
    cues: MessageWriter<'w, SpecialCue>,
}

impl Entry<'_, '_> {
    /// Plays a special's cue, for the kinds that announce their entry.
    pub fn cue(&mut self, cue: Cue, at: Vec3) {
        self.cues.write(SpecialCue { cue, at: Some(at) });
    }

    /// A Hellhound Spawn Point to strike at: one in an Active Zone picked as a Round's hound is,
    /// or, with none in an Active Zone, any on this Map. `None` if the Map has none.
    pub fn hellhound_point(&mut self) -> Option<Vec3> {
        let map = self.map.as_ref().map_or_else(MapId::default, |map| **map);
        let points = || {
            self.spawn_points
                .iter()
                .filter(|(_, point)| point.kind == SpawnKind::Hellhound && point.zone.map() == map)
                .map(|(t, point)| (point.zone, t.translation))
        };
        let anchors: Vec<Vec3> = self.players.spawn_anchors().iter().map(|spot| spot.centre).collect();
        let mut candidates = hellhound_spawn_candidates(points(), self.active.0, &anchors);
        if candidates.is_empty() {
            candidates = hellhound_spawn_candidates(points(), ZoneId::ALL.into_iter().collect(), &anchors);
        }
        (!candidates.is_empty()).then(|| candidates[rand::random_range(0..candidates.len())])
    }

    /// Where `at` puts a Zombie: the point and, through a window, which Barrier it climbs in by.
    /// `None` while there's no window to come through.
    pub fn resolve(&mut self, at: SpawnAt) -> Option<(Vec3, Option<BarrierId>)> {
        match at {
            SpawnAt::Point(point) => Some((point, None)),
            SpawnAt::BarrierInActiveZone => {
                let start = self.map.as_ref().map_or_else(MapId::default, |map| **map).layout().start_zone();
                let windows = window_candidates(
                    self.spawn_points.iter().filter_map(|(t, point)| match point.kind {
                        SpawnKind::Window(barrier) => Some((point.zone, (t.translation, barrier))),
                        _ => None,
                    }),
                    self.players.first().map_or(start, |player| player.zone),
                    self.active.0,
                );
                if windows.is_empty() {
                    return None;
                }
                let (point, barrier) = windows[rand::random_range(0..windows.len())];
                // Every Plank off in one blow.
                self.barriers.smash(barrier);
                self.cues.write(SpecialCue { cue: Cue::SapperSmash, at: Some(point) });
                Some((point, Some(barrier)))
            }
        }
    }
}

pub struct RequestPlugin;

impl Plugin for RequestPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SpawnZombie>().add_systems(
            Update,
            spawn_requested_walkers.run_if(on_message::<SpawnZombie>).in_set(HostSimulation).run_if(world_runs),
        );
    }
}

/// Each `SpawnZombie` for a Walker spawns a normal Zombie outside the Horde: standing where it's
/// asked for, or smashing in through a window. Without `holds_round` it carries over.
pub(crate) fn spawn_requested_walkers(
    mut commands: Commands,
    mut requests: MessageReader<SpawnZombie>,
    round: Option<Res<Round>>,
    mut entry: Entry,
    zombie_assets: ZombieAssetsParam,
) {
    let number = round.map_or(1, |round| round.number);
    for request in requests.read() {
        let Some(tier) = request.kind.normal_tier() else {
            continue;
        };
        let Some((at, barrier)) = entry.resolve(request.at) else {
            continue;
        };
        let health = requested_health(number, request.health_factor);
        let zombie = spawn_zombie(&mut commands, &zombie_assets, at, health, tier, None);
        let mut entity = commands.entity(zombie);
        entity.insert(Requested::of(request));
        if !request.holds_round {
            entity.insert(CarriesOver);
        }
        if let Some(heavyweight) = request.heavyweight {
            entity.insert(heavyweight);
        }
        if let Some(barrier) = barrier {
            entity.insert(Entering::new(barrier));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_requested_zombie_has_the_rounds_base_health_times_its_factor() {
        assert_eq!(requested_health(1, 1.0), 150.0);
        assert_eq!(requested_health(10, 30.0), 1045.0 * 30.0);
        assert_eq!(requested_health(30, 50.0), 7000.0 * 50.0);
    }

    /// Bevy checks a system's parameters for clashing access as it's set up, which would
    /// otherwise only show as a panic when the game starts.
    #[test]
    fn the_request_handlers_have_no_clashing_parameters() {
        fn set_up<M>(system: impl IntoSystem<(), (), M>) {
            let mut world = World::new();
            IntoSystem::into_system(system).initialize(&mut world);
        }
        set_up(spawn_requested_walkers);
        set_up(crate::enemies::special::spawn_requested_napalms);
        set_up(crate::enemies::special::napalm_trails);
        set_up(crate::enemies::sapper::spawn_requested_sappers);
        set_up(crate::enemies::hellhound::spawn_requested_hellhounds);
    }

    #[test]
    fn the_profile_defaults_to_classics_napalm() {
        let profile = SpawnProfile::default();
        assert!(profile.detonates);
        assert_eq!(profile.trail, None);
        assert!(profile.helmet_reforms_at.is_empty());
        assert_eq!(profile.sabotages_at_once, 1);
    }
}
