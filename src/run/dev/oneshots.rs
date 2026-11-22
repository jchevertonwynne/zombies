//! The world one-shots of the Cheat Panel (ADR 0010): Round jump, kill all, spawn, open all Doors
//! and Power on. Each is a neutral Classic message the Host sends for the Player who asked (a Round
//! to begin, every Zombie to kill Unpaid, a Zombie to spawn, every Door to open, the Power to
//! turn on); developer mode never touches the world itself.
//!
//! Two kinds of Spawn are Gauntlet's: a Boss (`SummonBoss`) and the Elite tick (`ELITE_TAG`).
//! Classic has neither, so a Host in a Classic Run drops them without a word.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::enemies::hellhound::hellhound_health;
use crate::enemies::nav::{NavGraph, NavState, Node};
use crate::enemies::request::{RequestedKind, SpawnAt, SpawnProfile, SpawnZombie};
use crate::enemies::target::{Players, path_distance};
use crate::enemies::zombie::KillAllZombies;
use crate::maps::barrier::Barriers;
use crate::maps::door::{Door, OpenAllDoors};
use crate::maps::power::SwitchPowerOn;
use crate::maps::shifting::{PieceState, standing_rests};
use crate::players::player::Player;
use crate::run::gauntlet::arc::{BossKind, SummonBoss};
use crate::run::gauntlet::elite::ELITE_TAG;
use crate::run::mode::Mode;
use crate::run::round::{BeginRound, Round, zombie_health};
use crate::tuning::{NAPALM_HEALTH_MULTIPLIER, NOVA_CRAWLER_HEALTH_MULT, SAPPER_HEALTH_MULTIPLIER, SHRIEKER_HEALTH_MULTIPLIER};

/// How far ahead of the Player, along the way walked, a spawned Zombie comes up.
pub const SPAWN_AHEAD_METRES: f32 = 10.0;

/// How a Round jump moves the Round.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoundJump {
    Back,
    Forward,
    Back5,
    Forward5,
    /// To the next Round that is a multiple of ten: Gauntlet's Boss Rounds.
    NextTenth,
}

impl RoundJump {
    pub fn label(self) -> &'static str {
        match self {
            RoundJump::Back => "Round -1",
            RoundJump::Forward => "Round +1",
            RoundJump::Back5 => "Round -5",
            RoundJump::Forward5 => "Round +5",
            RoundJump::NextTenth => "Round to next x10",
        }
    }

    /// The Round the jump goes to from Round `current`: never before Round 1.
    pub fn target(self, current: u32) -> u32 {
        match self {
            RoundJump::Back => current.saturating_sub(1),
            RoundJump::Forward => current.saturating_add(1),
            RoundJump::Back5 => current.saturating_sub(5),
            RoundJump::Forward5 => current.saturating_add(5),
            RoundJump::NextTenth => (current / 10 + 1).saturating_mul(10),
        }
        .max(1)
    }
}

/// What a Spawn Cheat can bring.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spawnable {
    Walker,
    Jogger,
    Sprinter,
    Napalm,
    Shrieker,
    NovaCrawler,
    Sapper,
    Hellhound,
    /// The three Bosses are Gauntlet's: in Classic a request for one is dropped.
    Bloater,
    PackAlpha,
    Warden,
}

impl Spawnable {
    pub const ALL: [Spawnable; 11] = [
        Spawnable::Walker,
        Spawnable::Jogger,
        Spawnable::Sprinter,
        Spawnable::Napalm,
        Spawnable::Shrieker,
        Spawnable::NovaCrawler,
        Spawnable::Sapper,
        Spawnable::Hellhound,
        Spawnable::Bloater,
        Spawnable::PackAlpha,
        Spawnable::Warden,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Spawnable::Walker => "Walker",
            Spawnable::Jogger => "Jogger",
            Spawnable::Sprinter => "Sprinter",
            Spawnable::Napalm => "Napalm",
            Spawnable::Shrieker => "Shrieker",
            Spawnable::NovaCrawler => "Nova Crawler",
            Spawnable::Sapper => "Sapper",
            Spawnable::Hellhound => "Hellhound",
            Spawnable::Bloater => "Bloater",
            Spawnable::PackAlpha => "Pack Alpha",
            Spawnable::Warden => "Warden",
        }
    }

    /// The Boss it is, if it is one.
    pub fn boss(self) -> Option<BossKind> {
        match self {
            Spawnable::Bloater => Some(BossKind::Bloater),
            Spawnable::PackAlpha => Some(BossKind::PackAlpha),
            Spawnable::Warden => Some(BossKind::Warden),
            _ => None,
        }
    }

    /// Whether the Elite tick can go on it: only an ordinary Zombie is ever an Elite.
    pub fn can_be_elite(self) -> bool {
        matches!(self, Spawnable::Walker | Spawnable::Jogger | Spawnable::Sprinter)
    }

    /// Whether it exists only in a Gauntlet Run.
    pub fn gauntlet_only(self) -> bool {
        self.boss().is_some()
    }

    /// The request for it at `at` in `round`, outside the Horde and the Alive Cap, not holding the
    /// Round open. Its health is its kind's own for that Round. `None` for a Boss, which is a
    /// `SummonBoss`.
    pub fn request(self, round: &Round, at: Vec3, elite: bool) -> Option<SpawnZombie> {
        let (kind, health_factor) = match self {
            Spawnable::Walker => (RequestedKind::Walker, 1.0),
            Spawnable::Jogger => (RequestedKind::Jogger, 1.0),
            Spawnable::Sprinter => (RequestedKind::Sprinter, 1.0),
            Spawnable::Napalm => (RequestedKind::Napalm, NAPALM_HEALTH_MULTIPLIER),
            Spawnable::Shrieker => (RequestedKind::Shrieker, SHRIEKER_HEALTH_MULTIPLIER),
            Spawnable::NovaCrawler => (RequestedKind::NovaCrawler, NOVA_CRAWLER_HEALTH_MULT),
            Spawnable::Sapper => (RequestedKind::Sapper, SAPPER_HEALTH_MULTIPLIER),
            Spawnable::Hellhound => {
                // The health of the Round's own hounds: this Round's, or the latest tier's.
                let tier = round.hellhound.unwrap_or_else(|| round.hellhound_tier());
                (RequestedKind::Hellhound, hellhound_health(tier) / zombie_health(round.number) as f32)
            }
            Spawnable::Bloater | Spawnable::PackAlpha | Spawnable::Warden => return None,
        };
        Some(SpawnZombie {
            kind,
            health_factor,
            scale: 1.0,
            at: SpawnAt::Point(at),
            holds_round: false,
            profile: SpawnProfile::default(),
            heavyweight: None,
            tag: (elite && self.can_be_elite()).then_some(ELITE_TAG),
            outside_cap: true,
        })
    }
}

/// A Spawn Cheat: what to bring, and whether it is an Elite.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spawn {
    pub kind: Spawnable,
    /// The Elite tick: an Elite of any ordinary Zombie. Gauntlet's; in Classic it does nothing.
    pub elite: bool,
}

impl Spawn {
    pub fn label(self) -> &'static str {
        self.kind.label()
    }
}

/// Where a Zombie spawned ahead of a Player at `feet` facing `forward` comes up: the path node,
/// reachable on foot (`nav`), whose walk from the Player is nearest `SPAWN_AHEAD_METRES`, with a
/// pull toward the way they face. Never a Vent, never a node outside every Zone, never right
/// beside them.
pub fn spot_ahead(graph: &NavGraph, feet: Vec3, forward: Vec3, nav: NavState) -> Option<Vec3> {
    let facing = Vec2::new(forward.x, forward.z).normalize_or_zero();
    graph
        .nodes
        .iter()
        .filter(|node: &&Node| !node.vent && node.zone.is_some())
        .filter_map(|node| {
            let walk = path_distance(graph, feet, node.at, nav)?;
            let flat = Vec2::new(node.at.x - feet.x, node.at.z - feet.z);
            if walk < 3.0 || flat.length() < 3.0 {
                return None;
            }
            let ahead = flat.normalize_or_zero().dot(facing);
            Some((node.at, (walk - SPAWN_AHEAD_METRES).abs() + (1.0 - ahead) * 6.0))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(at, _)| at)
}

/// What the world one-shots need: the messages they send, and what the spawn looks at to place a
/// Zombie.
#[derive(SystemParam)]
pub struct WorldCheats<'w, 's> {
    round: Option<Res<'w, Round>>,
    mode: Option<Res<'w, Mode>>,
    graph: Option<Res<'w, NavGraph>>,
    barriers: Option<Res<'w, Barriers>>,
    doors: Query<'w, 's, &'static Door>,
    pieces: Query<'w, 's, &'static PieceState>,
    players: Players<'w, 's>,
    bodies: Query<'w, 's, &'static Transform, With<Player>>,
    begin: MessageWriter<'w, BeginRound>,
    kill_all: MessageWriter<'w, KillAllZombies>,
    open: MessageWriter<'w, OpenAllDoors>,
    power: MessageWriter<'w, SwitchPowerOn>,
    spawns: MessageWriter<'w, SpawnZombie>,
    bosses: MessageWriter<'w, SummonBoss>,
}

impl WorldCheats<'_, '_> {
    /// Whether a Run is a Gauntlet's.
    fn gauntlet(&self) -> bool {
        self.mode.as_deref() == Some(&Mode::Gauntlet)
    }

    /// Where a Zombie spawned ahead of `player` comes up, if there is anywhere to.
    fn ahead_of(&self, player: Entity) -> Option<Vec3> {
        let graph = self.graph.as_ref()?;
        let spot = self.players.iter().find(|spot| spot.entity == player)?;
        let forward = self.bodies.get(player).map_or(Vec3::NEG_Z, |body| *body.forward());
        let open_doors: Vec<_> = self.doors.iter().filter(|door| door.open).filter_map(Door::route).collect();
        let rests = standing_rests(self.pieces.iter().map(|state| (state.id(), state.phase())), graph.map.layout().shifting.len());
        let barrier_open = |barrier| self.barriers.as_ref().is_some_and(|barriers| barriers.is_open(barrier));
        spot_ahead(graph, spot.feet, forward, NavState::new(&open_doors, &barrier_open).with_rests(&rests))
    }

    /// Does a world one-shot for `player`, if `cheat` is one. `None` if it is not, `Some(true)` if
    /// it was done (so it taints the Run) and `Some(false)` if it was a request that can't be done
    /// here and was dropped.
    pub fn apply(&mut self, player: Entity, cheat: super::Cheat) -> Option<bool> {
        use super::Cheat;
        Some(match cheat {
            Cheat::RoundJump(jump) => {
                let Some(round) = self.round.as_deref() else { return Some(false) };
                let target = jump.target(round.number);
                if target == round.number && round.phase == crate::run::round::RoundPhase::Active {
                    return Some(false);
                }
                self.begin.write(BeginRound(target));
                true
            }
            Cheat::KillAll => {
                self.kill_all.write(KillAllZombies);
                true
            }
            Cheat::OpenAllDoors => {
                self.open.write(OpenAllDoors);
                true
            }
            Cheat::PowerOn => {
                self.power.write(SwitchPowerOn);
                true
            }
            Cheat::Spawn(Spawn { kind, elite }) => {
                if kind.gauntlet_only() && !self.gauntlet() {
                    return Some(false);
                }
                let Some(at) = self.ahead_of(player) else { return Some(false) };
                let elite = elite && self.gauntlet();
                match kind.boss() {
                    Some(boss) => {
                        self.bosses.write(SummonBoss { kind: boss, at });
                    }
                    None => {
                        let Some(round) = self.round.as_deref() else { return Some(false) };
                        let Some(request) = kind.request(round, at, elite) else { return Some(false) };
                        self.spawns.write(request);
                    }
                }
                true
            }
            #[allow(unreachable_patterns, reason = "the other Cheats are not the world's")]
            _ => return None,
        })
    }
}

/// Registers the messages the world one-shots send, for an app that has not got the plugins that do.
pub fn add_messages(app: &mut App) {
    app.add_message::<BeginRound>()
        .add_message::<KillAllZombies>()
        .add_message::<OpenAllDoors>()
        .add_message::<SwitchPowerOn>()
        .add_message::<SpawnZombie>()
        .add_message::<SummonBoss>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maps::map::MapId;

    #[test]
    fn a_round_jump_goes_where_it_says_and_never_before_round_one() {
        assert_eq!(RoundJump::Forward.target(7), 8);
        assert_eq!(RoundJump::Back.target(7), 6);
        assert_eq!(RoundJump::Forward5.target(7), 12);
        assert_eq!(RoundJump::Back5.target(7), 2);
        assert_eq!(RoundJump::Back5.target(3), 1);
        assert_eq!(RoundJump::Back.target(1), 1);
        assert_eq!(RoundJump::NextTenth.target(1), 10);
        assert_eq!(RoundJump::NextTenth.target(10), 20);
        assert_eq!(RoundJump::NextTenth.target(19), 20);
    }

    #[test]
    fn only_ordinary_zombies_take_the_elite_tick_and_only_bosses_are_gauntlets() {
        let round = Round::in_progress(12);
        for kind in Spawnable::ALL {
            let request = kind.request(&round, Vec3::ZERO, true);
            assert_eq!(request.is_none(), kind.boss().is_some(), "{kind:?}");
            if let Some(request) = request {
                assert_eq!(request.tag, kind.can_be_elite().then_some(ELITE_TAG), "{kind:?}");
                assert!(request.outside_cap && !request.holds_round, "{kind:?} stays outside the Horde and the cap");
            }
            assert_eq!(kind.gauntlet_only(), kind.boss().is_some());
        }
    }

    #[test]
    fn a_hellhound_spawns_at_the_rounds_hound_health() {
        let round = Round::in_progress(12);
        let request = Spawnable::Hellhound.request(&round, Vec3::ZERO, false).unwrap();
        let health = crate::enemies::request::requested_health(12, request.health_factor);
        assert!((health - hellhound_health(round.hellhound_tier())).abs() < 0.01);
    }

    #[test]
    fn a_spawn_comes_up_about_ten_metres_along_the_way_walked_and_ahead() {
        let graph = NavGraph::new(MapId::Greybox);
        let doors: Vec<_> = graph
            .edges
            .iter()
            .filter_map(|edge| match edge.gate {
                crate::enemies::nav::Gate::Door(a, b) => Some((a, b)),
                _ => None,
            })
            .collect();
        let open = |_| true;
        let nav = NavState::new(&doors, &open);
        let start = graph.nodes.iter().find(|node| !node.vent && node.zone.is_some()).unwrap().at;
        let mut found = 0;
        for heading in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
            let Some(at) = spot_ahead(&graph, start, heading, nav) else { continue };
            found += 1;
            let walk = path_distance(&graph, start, at, nav).unwrap();
            assert!((3.0..=25.0).contains(&walk), "walked {walk} m");
            assert!(graph.nodes.iter().any(|node| node.at == at && !node.vent && node.zone.is_some()));
        }
        assert!(found > 0, "somewhere on the Greybox is ten metres on");
    }
}
