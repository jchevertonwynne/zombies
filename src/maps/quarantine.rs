//! The Quarantine: the Sanatorium's clock that seals its Wings one at a time. Once the Power is on it
//! runs an All Clear, then a Warning, the Swing that shuts a Wing's Quarantine Doors, the minute
//! it stays Sealed, a Release and the Swing that opens them, and round again with the next Wing
//! that has a bought doorway. It holds at All Clear through a Gauntlet Boss Round.
//!
//! The Host decides everything; the Pieces (the Quarantine Doors) are driven from the stage in
//! `shifting` and told over the wire as any Piece is. Every machine shows the stage the Host last
//! told and runs its seconds on only to show them. It runs on world time, so a solo Offer pauses it
//! and an Intermission does not.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::audio::PlayCue;
use crate::audio::cues::Cue;
use crate::authority::{Authority, HostSimulation};
use crate::maps::door::Door;
use crate::maps::map::MapId;
use crate::maps::power::Power;
use crate::maps::shifting::{Driver, door_centre, gate_open, pieces};
use crate::run::gauntlet::arc::is_boss_round;
use crate::run::mode::Mode;
use crate::run::round::{Round, RoundPhase};
use crate::state::{GameState, world_runs};
use crate::tuning::*;

/// A Wing's place in `Layout.wings`, the same on every machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct WingId(pub u8);

/// Where the Quarantine is in its cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Stage {
    /// Before the Power: every Quarantine Door stands Open and nothing runs.
    #[default]
    Dormant,
    /// Every Wing open, between one Wing opening and the next Warning.
    AllClear,
    /// A Wing's klaxon sounds and its beacons spin; its doors still stand Open.
    Warning,
    /// The Wing's doors swing shut.
    Sealing,
    Sealed,
    /// The Wing's beacons turn green and a chime sounds; its doors still stand shut.
    Release,
    /// The Wing's doors swing open.
    Opening,
}

impl Stage {
    /// How long the stage lasts, or `None` if it waits on something else.
    pub fn secs(self) -> Option<f32> {
        match self {
            Stage::Dormant => None,
            Stage::AllClear => Some(QUARANTINE_ALL_CLEAR_SECS),
            Stage::Warning => Some(QUARANTINE_WARNING_SECS),
            Stage::Sealing | Stage::Opening => Some(SWING_SECS),
            Stage::Sealed => Some(QUARANTINE_SEALED_SECS),
            Stage::Release => Some(QUARANTINE_RELEASE_SECS),
        }
    }

    /// The stage after it. A Wing's last stage hands back to All Clear.
    pub fn next(self) -> Stage {
        match self {
            Stage::Dormant => Stage::AllClear,
            Stage::AllClear => Stage::Warning,
            Stage::Warning => Stage::Sealing,
            Stage::Sealing => Stage::Sealed,
            Stage::Sealed => Stage::Release,
            Stage::Release => Stage::Opening,
            Stage::Opening => Stage::AllClear,
        }
    }

    /// Whether a Wing is out of All Clear.
    #[cfg(test)]
    pub fn wing_out(self) -> bool {
        !matches!(self, Stage::Dormant | Stage::AllClear)
    }

    /// Whether the Wing's doors are to stand shut, or be swinging shut.
    pub fn seals(self) -> bool {
        matches!(self, Stage::Sealing | Stage::Sealed | Stage::Release)
    }

    /// Whether the klaxon sounds and the beacons spin: through Warning and Sealing.
    pub fn alarm(self) -> bool {
        matches!(self, Stage::Warning | Stage::Sealing)
    }
}

/// What the Host knows that the clock reads.
#[derive(Debug, Clone, Copy)]
pub struct Conditions<'a> {
    /// The Power is on.
    pub power: bool,
    /// A Gauntlet Boss Round is being played.
    pub boss: bool,
    /// For each Wing, whether any of its doorways has been bought.
    pub bought: &'a [bool],
}

/// The Quarantine. The Host runs it; every other machine shows what the Host told it.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct Quarantine {
    stage: Stage,
    /// The Wing out of All Clear, if one is.
    wing: Option<WingId>,
    /// Seconds into the stage.
    elapsed: f32,
    /// The Wing the next Warning tries first.
    next_wing: u8,
}

impl Quarantine {
    pub fn stage(&self) -> Stage {
        self.stage
    }

    pub fn wing(&self) -> Option<WingId> {
        self.wing
    }

    pub fn elapsed(&self) -> f32 {
        self.elapsed
    }

    /// Shows the stage the Host says it is in.
    pub fn show(&mut self, stage: Stage, wing: Option<WingId>, elapsed: f32) {
        self.stage = stage;
        self.wing = wing;
        self.elapsed = elapsed.max(0.0);
    }

    /// Runs the shown stage's seconds on, only so it can be shown: it never leaves its stage until
    /// the Host says what comes next.
    pub fn run_shown(&mut self, dt: f32) {
        if let Some(secs) = self.stage.secs() {
            self.elapsed = (self.elapsed + dt).min(secs);
        }
    }

    fn enter(&mut self, stage: Stage) {
        self.stage = stage;
        self.elapsed = 0.0;
        if matches!(stage, Stage::AllClear) {
            self.wing = None;
        }
    }

    /// The next Wing with a bought doorway, in Layout order after the last one taken.
    fn pick(&mut self, bought: &[bool]) -> Option<WingId> {
        let count = bought.len();
        (0..count).map(|i| (usize::from(self.next_wing) + i) % count).find(|&wing| bought[wing]).map(|wing| {
            self.next_wing = ((wing + 1) % count) as u8;
            WingId(wing as u8)
        })
    }

    /// Runs the clock `dt` seconds on. Returns whether the stage changed.
    pub fn advance(&mut self, dt: f32, conditions: Conditions) -> bool {
        let before = (self.stage, self.wing);
        let mut left = dt;
        loop {
            if conditions.boss {
                match self.stage {
                    Stage::Warning => self.enter(Stage::AllClear),
                    Stage::Sealed => self.enter(Stage::Release),
                    _ => {}
                }
            }
            match self.stage {
                Stage::Dormant => {
                    if !conditions.power {
                        break;
                    }
                    self.enter(Stage::AllClear);
                }
                // The clock holds at All Clear through a Boss Round.
                Stage::AllClear if conditions.boss => break,
                stage => {
                    let secs = stage.secs().unwrap_or(0.0);
                    if self.elapsed + left < secs {
                        self.elapsed += left;
                        break;
                    }
                    left -= secs - self.elapsed;
                    self.elapsed = secs;
                    match stage {
                        // With no doorway bought there is nothing to seal: another All Clear.
                        Stage::AllClear => match self.pick(conditions.bought) {
                            Some(wing) => {
                                self.enter(Stage::Warning);
                                self.wing = Some(wing);
                            }
                            None => self.enter(Stage::AllClear),
                        },
                        stage => self.enter(stage.next()),
                    }
                }
            }
        }
        (self.stage, self.wing) != before
    }

    /// The Developer mode step: straight to the next stage, picking a Wing at All Clear even if no
    /// doorway is bought.
    pub fn step_now(&mut self, conditions: Conditions) {
        match self.stage {
            Stage::AllClear => {
                let wing = self.pick(conditions.bought).or_else(|| self.pick(&vec![true; conditions.bought.len()]));
                if let Some(wing) = wing {
                    self.enter(Stage::Warning);
                    self.wing = Some(wing);
                }
            }
            stage => self.enter(stage.next()),
        }
    }

    /// The Wing's Quarantine Doors are to stand shut (or be Swinging shut), at this stage.
    pub fn seals(&self, wing: WingId) -> bool {
        self.wing == Some(wing) && self.stage.seals()
    }
}

/// Request to step the Quarantine to its next stage now. The Host judges it.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepQuarantine;

/// The Host's systems that run the Quarantine's clock.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct QuarantineClock;

/// For each Wing of `map`, whether any of its Quarantine Doors' gate Doors is open.
pub fn bought_wings(map: MapId, doors: &Query<&Door>) -> Vec<bool> {
    let layout = map.layout();
    let mut bought = vec![false; layout.wings.len()];
    for (_, piece) in pieces(layout) {
        if let Driver::Quarantine(WingId(wing)) = piece.driver
            && let Some(slot) = bought.get_mut(usize::from(wing))
            && gate_open(doors, piece.door)
        {
            *slot = true;
        }
    }
    bought
}

/// Whether a Gauntlet Boss Round is being played: from its start until it is cleared.
fn boss_round(mode: Option<&Mode>, round: Option<&Round>) -> bool {
    mode == Some(&Mode::Gauntlet) && round.is_some_and(|round| round.phase == RoundPhase::Active && is_boss_round(round.number))
}

/// Back to Dormant as a Run begins.
fn reset(mut quarantine: ResMut<Quarantine>) {
    *quarantine = Quarantine::default();
}

/// The Host's clock: Dormant until the Power, then the Wings in turn.
fn run_clock(
    mut quarantine: ResMut<Quarantine>,
    mut steps: MessageReader<StepQuarantine>,
    power: Option<Res<Power>>,
    mode: Option<Res<Mode>>,
    round: Option<Res<Round>>,
    doors: Query<&Door>,
    map: Option<Res<MapId>>,
    time: Res<Time>,
) {
    let stepped = steps.read().count() > 0;
    let Some(map) = map.filter(|map| !map.layout().wings.is_empty()) else { return };
    let bought = bought_wings(*map, &doors);
    let conditions = Conditions {
        power: power.is_some_and(|power| power.is_on()),
        boss: boss_round(mode.as_deref(), round.as_deref()),
        bought: &bought,
    };
    let mut next = *quarantine;
    if stepped && next.stage != Stage::Dormant {
        next.step_now(conditions);
    }
    next.advance(time.delta_secs(), conditions);
    // Written only on a change of stage or seconds, so `Changed` means something.
    if next != *quarantine {
        *quarantine = next;
    }
}

/// A client's clock: runs the seconds of the stage it was told on, to show them.
fn run_shown(mut quarantine: ResMut<Quarantine>, time: Res<Time>) {
    quarantine.run_shown(time.delta_secs());
}

/// The sounds that mark a stage, on every machine, from the stage it shows: the siren once at the
/// Warning, and the chime at the Release. (The klaxon loops, the doors' swing and slam sound from the
/// Pieces.)
fn quarantine_cues(quarantine: Res<Quarantine>, map: Option<Res<MapId>>, mut last: Local<Stage>, mut cues: MessageWriter<PlayCue>) {
    if quarantine.stage == *last {
        return;
    }
    *last = quarantine.stage;
    let Some(layout) = map.map(|map| map.layout()) else { return };
    match quarantine.stage {
        Stage::Warning => {
            cues.write(PlayCue::new(Cue::QuarantineSiren, None));
        }
        Stage::Release => {
            for (_, piece) in pieces(layout) {
                if matches!(piece.driver, Driver::Quarantine(wing) if Some(wing) == quarantine.wing) {
                    cues.write(PlayCue::new(Cue::QuarantineRelease, Some(door_centre(piece))));
                }
            }
        }
        _ => {}
    }
}

pub struct QuarantinePlugin;

impl Plugin for QuarantinePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Quarantine>()
            .add_message::<StepQuarantine>()
            .add_message::<PlayCue>()
            .add_systems(OnEnter(GameState::Playing), reset)
            .add_systems(Update, run_clock.in_set(QuarantineClock).in_set(HostSimulation).run_if(world_runs))
            .add_systems(
                Update,
                (run_shown.run_if(not(in_state(Authority::Host))), quarantine_cues).chain().after(QuarantineClock).run_if(world_runs),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [bool; 3] = [true; 3];

    fn at_power(bought: &[bool]) -> Conditions<'_> {
        Conditions { power: true, boss: false, bought }
    }

    /// Runs `quarantine` on in whole-second steps and lists the stage at the end of each.
    fn run(quarantine: &mut Quarantine, seconds: u32, conditions: Conditions) -> Vec<(Stage, Option<WingId>)> {
        (0..seconds)
            .map(|_| {
                quarantine.advance(1.0, conditions);
                (quarantine.stage(), quarantine.wing())
            })
            .collect()
    }

    #[test]
    fn quarantine_stage_timeline_runs_113_seconds_a_wing() {
        assert_eq!((QUARANTINE_ALL_CLEAR_SECS, QUARANTINE_WARNING_SECS, SWING_SECS, QUARANTINE_SEALED_SECS, QUARANTINE_RELEASE_SECS), (30.0, 10.0, 5.0, 60.0, 3.0));
        let total: f32 = [Stage::AllClear, Stage::Warning, Stage::Sealing, Stage::Sealed, Stage::Release, Stage::Opening].iter().map(|stage| stage.secs().unwrap()).sum();
        assert_eq!(total, 113.0);
        let mut quarantine = Quarantine::default();
        // The Power starts it with an All Clear.
        quarantine.advance(0.0, at_power(&ALL));
        assert_eq!(quarantine.stage(), Stage::AllClear);
        let stages = run(&mut quarantine, 113, at_power(&ALL));
        // Second n of the cycle ends in the stage it has just entered, so each boundary is exact.
        let first_of = |stage: Stage| stages.iter().position(|(s, _)| *s == stage).unwrap() + 1;
        assert_eq!(first_of(Stage::Warning), 30);
        assert_eq!(first_of(Stage::Sealing), 40);
        assert_eq!(first_of(Stage::Sealed), 45);
        assert_eq!(first_of(Stage::Release), 105);
        assert_eq!(first_of(Stage::Opening), 108);
        assert_eq!(stages[112], (Stage::AllClear, None), "113 s on, back to All Clear with no Wing out");
        assert!(stages[29..112].iter().all(|(_, wing)| *wing == Some(WingId(0))), "the first Wing");
        assert_eq!(quarantine.elapsed(), 0.0);
    }

    #[test]
    fn a_long_step_carries_its_seconds_through_every_stage() {
        let mut quarantine = Quarantine::default();
        quarantine.advance(113.0 + 30.0 + 2.5, at_power(&ALL));
        assert_eq!((quarantine.stage(), quarantine.wing()), (Stage::Warning, Some(WingId(1))));
        assert!((quarantine.elapsed() - 2.5).abs() < 1e-4);
    }

    #[test]
    fn quarantine_is_dormant_until_the_power() {
        let mut quarantine = Quarantine::default();
        let off = Conditions { power: false, boss: false, bought: &ALL };
        assert!(run(&mut quarantine, 600, off).iter().all(|&(stage, wing)| (stage, wing) == (Stage::Dormant, None)));
        assert!(!quarantine.advance(0.0, off));
        assert!(quarantine.advance(0.1, at_power(&ALL)), "the Power starts it");
        assert_eq!(quarantine.stage(), Stage::AllClear);
        assert!((quarantine.elapsed() - 0.1).abs() < 1e-5, "with an All Clear that starts from the Power");
    }

    #[test]
    fn quarantine_skips_wings_with_no_bought_doorway() {
        let mut quarantine = Quarantine::default();
        // Only the Isolation Wing (2) has a doorway bought: it is taken each time round.
        let only_last = [false, false, true];
        let mut taken = Vec::new();
        for _ in 0..3 {
            quarantine.advance(30.0 + 0.5, at_power(&only_last));
            taken.push(quarantine.wing());
            quarantine.advance(113.0 - 30.0, at_power(&only_last));
        }
        assert_eq!(taken, [Some(WingId(2)); 3]);
        // With the East Wing (1) and the Isolation Wing bought, they take turns, West skipped.
        let mut quarantine = Quarantine::default();
        let some = [false, true, true];
        let mut taken = Vec::new();
        for _ in 0..4 {
            quarantine.advance(30.5, at_power(&some));
            taken.push(quarantine.wing());
            quarantine.advance(113.0 - 30.0, at_power(&some));
        }
        assert_eq!(taken, [Some(WingId(1)), Some(WingId(2)), Some(WingId(1)), Some(WingId(2))]);
        // All three bought: Layout order, round and round.
        let mut quarantine = Quarantine::default();
        let mut taken = Vec::new();
        for _ in 0..4 {
            quarantine.advance(30.5, at_power(&ALL));
            taken.push(quarantine.wing().unwrap().0);
            quarantine.advance(113.0 - 30.0, at_power(&ALL));
        }
        assert_eq!(taken, [0, 1, 2, 0]);
    }

    #[test]
    fn quarantine_repeats_all_clear_when_no_wing_is_bought() {
        let mut quarantine = Quarantine::default();
        let none = [false; 3];
        let stages = run(&mut quarantine, 400, at_power(&none));
        assert!(stages.iter().all(|&(stage, wing)| (stage, wing) == (Stage::AllClear, None)), "never a Warning");
        // Buying a doorway lets the next All Clear's end start a Warning.
        quarantine.advance(25.0, at_power(&ALL));
        assert_eq!((quarantine.stage(), quarantine.wing()), (Stage::Warning, Some(WingId(0))));
    }

    #[test]
    fn boss_round_drops_a_warning_to_all_clear_and_releases_a_sealed_wing() {
        let boss = Conditions { power: true, boss: true, bought: &ALL };
        // A Warning drops straight back to All Clear, with no Wing out.
        let mut quarantine = Quarantine::default();
        quarantine.advance(30.0 + 4.0, at_power(&ALL));
        assert_eq!((quarantine.stage(), quarantine.wing()), (Stage::Warning, Some(WingId(0))));
        assert!(quarantine.advance(0.0, boss));
        assert_eq!((quarantine.stage(), quarantine.wing(), quarantine.elapsed()), (Stage::AllClear, None, 0.0));
        // A Sealed Wing goes straight to Release, and its doors then open as they always do.
        let mut quarantine = Quarantine::default();
        quarantine.advance(30.0 + 10.0 + 5.0 + 20.0, at_power(&ALL));
        assert_eq!(quarantine.stage(), Stage::Sealed);
        assert!(quarantine.advance(0.0, boss));
        assert_eq!((quarantine.stage(), quarantine.wing(), quarantine.elapsed()), (Stage::Release, Some(WingId(0)), 0.0));
        quarantine.advance(QUARANTINE_RELEASE_SECS, boss);
        assert_eq!(quarantine.stage(), Stage::Opening);
        quarantine.advance(SWING_SECS, boss);
        assert_eq!((quarantine.stage(), quarantine.wing()), (Stage::AllClear, None));
    }

    #[test]
    fn quarantine_holds_all_clear_through_a_boss_round() {
        let boss = Conditions { power: true, boss: true, bought: &ALL };
        let mut quarantine = Quarantine::default();
        quarantine.advance(0.0, at_power(&ALL));
        quarantine.advance(12.0, at_power(&ALL));
        assert!(run(&mut quarantine, 600, boss).iter().all(|&(stage, wing)| (stage, wing) == (Stage::AllClear, None)), "ten minutes of Boss Round, no Warning");
        // The Power coming on during a Boss Round holds there too.
        let mut fresh = Quarantine::default();
        assert!(run(&mut fresh, 300, boss).iter().all(|&(stage, _)| stage == Stage::AllClear));
        // Once the Round is cleared the clock runs again, from where it was held.
        let before = quarantine.elapsed();
        quarantine.advance(QUARANTINE_ALL_CLEAR_SECS - before, at_power(&ALL));
        assert_eq!(quarantine.stage(), Stage::Warning);
    }

    #[test]
    fn never_two_wings_out_of_all_clear() {
        // One Wing is out at a time by construction: the stage names one Wing, and a Wing is only
        // ever named between its Warning and its Opening.
        let mut quarantine = Quarantine::default();
        let mut seen = [0u32; 3];
        for _ in 0..(113 * 7) {
            quarantine.advance(1.0, at_power(&ALL));
            match (quarantine.stage().wing_out(), quarantine.wing()) {
                (true, Some(WingId(wing))) => seen[usize::from(wing)] += 1,
                (false, None) => {}
                other => panic!("{:?} names {other:?}", quarantine.stage()),
            }
        }
        assert!(seen.iter().all(|&n| n > 0), "every Wing took its turn: {seen:?}");
        // A Wing's turn is Warning to Opening: 10 + 5 + 60 + 3 + 5 = 83 s, then 30 s of All Clear.
        assert_eq!(seen[0], 83 * 3);
    }

    #[test]
    fn the_shown_stage_runs_its_seconds_on_but_never_leaves() {
        let mut shown = Quarantine::default();
        shown.show(Stage::Sealed, Some(WingId(1)), 50.0);
        shown.run_shown(20.0);
        assert_eq!((shown.stage(), shown.wing(), shown.elapsed()), (Stage::Sealed, Some(WingId(1)), QUARANTINE_SEALED_SECS));
        shown.show(Stage::Dormant, None, 0.0);
        shown.run_shown(5.0);
        assert_eq!(shown.elapsed(), 0.0);
    }

    #[test]
    fn the_developer_step_goes_to_the_next_stage_and_takes_a_wing_at_all_clear() {
        let mut quarantine = Quarantine::default();
        quarantine.show(Stage::AllClear, None, 3.0);
        let none = [false; 3];
        quarantine.step_now(at_power(&none));
        assert_eq!((quarantine.stage(), quarantine.wing()), (Stage::Warning, Some(WingId(0))), "even with nothing bought");
        for stage in [Stage::Sealing, Stage::Sealed, Stage::Release, Stage::Opening, Stage::AllClear] {
            quarantine.step_now(at_power(&none));
            assert_eq!(quarantine.stage(), stage);
        }
        assert_eq!(quarantine.wing(), None);
    }

    // ---- Headless, on the Sanatorium ----

    use bevy::ecs::system::RunSystemOnce;

    use crate::economy::buy::Purchased;
    use crate::enemies::nav::NavGraph;
    use crate::enemies::zombie::Zombie;
    use crate::enemies::zombie::headless::Sim;
    use crate::maps::shifting::{PieceId, PiecePhase, PieceState, ShiftingPlugin, outside_landing};
    use crate::maps::zone::{ActiveZones, PlayerZone, ZoneId, update_active_zones};
    use crate::players::last_stand::LastStand;
    use crate::players::player::{Player, Stance, Teleport};

    fn sanatorium_sim() -> Sim {
        Sim::on(MapId::Sanatorium, 20.0, |app| {
            app.add_plugins(ShiftingPlugin)
                .insert_resource(ActiveZones(Default::default()))
                .init_resource::<Power>()
                .add_message::<Purchased>()
                .add_message::<PlayCue>()
                .add_message::<Teleport>();
        })
    }

    fn show(sim: &mut Sim, stage: Stage, wing: Option<u8>, elapsed: f32) {
        sim.world_mut().resource_mut::<Quarantine>().show(stage, wing.map(WingId), elapsed);
    }

    fn show_piece(sim: &mut Sim, id: u8, phase: PiecePhase) {
        let world = sim.world_mut();
        world.query::<&mut PieceState>().iter_mut(world).find(|state| state.id() == PieceId(id)).unwrap().show(phase);
    }

    fn open_every_door(sim: &mut Sim) {
        let world = sim.world_mut();
        let mut doors = world.query::<&mut Door>();
        for mut door in doors.iter_mut(world) {
            door.open = true;
        }
    }

    fn phase(sim: &mut Sim, id: u8) -> PiecePhase {
        let world = sim.world_mut();
        world.query::<&PieceState>().iter(world).find(|state| state.id() == PieceId(id)).unwrap().phase()
    }

    fn put_player(sim: &mut Sim, feet: Vec3) {
        let world = sim.world_mut();
        let half = Stance::Standing.half_height();
        let mut query = world.query_filtered::<&mut Transform, With<Player>>();
        query.iter_mut(world).next().unwrap().translation = feet + Vec3::Y * half;
    }

    fn feet_of(sim: &mut Sim, entity: Entity) -> Vec3 {
        sim.world_mut().get::<Transform>(entity).unwrap().translation - Vec3::Y * Stance::Standing.half_height()
    }

    fn the_player(sim: &mut Sim) -> Entity {
        let world = sim.world_mut();
        world.query_filtered::<Entity, With<Player>>().iter(world).next().unwrap()
    }

    fn landing(piece: u8) -> Vec3 {
        let graph = NavGraph::new(MapId::Sanatorium);
        outside_landing(&graph, PieceId(piece), &MapId::Sanatorium.layout().shifting[usize::from(piece)]).unwrap()
    }

    /// The Power starts the Quarantine, which shuts a Wing's two doors over a Swing and opens them
    /// again, leaving the other Wings' alone.
    #[test]
    fn the_clock_swings_a_wings_doors_shut_and_open_and_no_others() {
        let mut sim = sanatorium_sim();
        open_every_door(&mut sim);
        sim.run(sim.frames(60.0));
        assert_eq!(sim.world_mut().resource::<Quarantine>().stage(), Stage::Dormant, "no Power, no Quarantine");
        assert!((0..6).all(|id| phase(&mut sim, id) == PiecePhase::Rest(0)));
        sim.world_mut().resource_mut::<Power>().switch_on();
        sim.run(sim.frames(30.5));
        assert_eq!(sim.world_mut().resource::<Quarantine>().stage(), Stage::Warning, "30 s of All Clear from the Power");
        assert_eq!(sim.world_mut().resource::<Quarantine>().wing(), Some(WingId(0)));
        assert!((0..6).all(|id| phase(&mut sim, id) == PiecePhase::Rest(0)), "the doors stand Open through the Warning");
        sim.run(sim.frames(10.0));
        assert_eq!(sim.world_mut().resource::<Quarantine>().stage(), Stage::Sealing);
        for id in 0..2 {
            assert!(matches!(phase(&mut sim, id), PiecePhase::Swinging { from: 0, to: 1, .. }), "{:?}", phase(&mut sim, id));
        }
        assert!((2..6).all(|id| phase(&mut sim, id) == PiecePhase::Rest(0)));
        sim.run(sim.frames(SWING_SECS + 0.5));
        assert_eq!(sim.world_mut().resource::<Quarantine>().stage(), Stage::Sealed);
        assert!((0..2).all(|id| phase(&mut sim, id) == PiecePhase::Rest(1)), "Sealed");
        sim.run(sim.frames(QUARANTINE_SEALED_SECS + QUARANTINE_RELEASE_SECS - 0.5));
        assert_eq!(sim.world_mut().resource::<Quarantine>().stage(), Stage::Opening);
        assert!(matches!(phase(&mut sim, 0), PiecePhase::Swinging { from: 1, to: 0, .. }));
        sim.run(sim.frames(SWING_SECS + 0.5));
        assert!((0..6).all(|id| phase(&mut sim, id) == PiecePhase::Rest(0)), "Open again");
        assert_eq!(sim.world_mut().resource::<Quarantine>().stage(), Stage::AllClear);
    }

    /// Never two Wings move or stand shut at once, across a whole round of the Wings.
    #[test]
    fn a_full_round_of_the_wings_never_has_two_wings_out_of_all_clear() {
        let mut sim = sanatorium_sim();
        open_every_door(&mut sim);
        sim.world_mut().resource_mut::<Power>().switch_on();
        let mut turns = [false; 3];
        for _ in 0..sim.frames(3.0 * 113.0 + 40.0) {
            sim.run(1);
            let out: Vec<usize> = (0..3).filter(|&wing| (0..2).any(|door| phase(&mut sim, (wing * 2 + door) as u8) != PiecePhase::Rest(0))).collect();
            assert!(out.len() <= 1, "Wings {out:?} are out of All Clear at once");
            for wing in out {
                turns[wing] = true;
            }
        }
        assert_eq!(turns, [true; 3], "every Wing was Sealed in its turn");
    }

    /// A Player inside a Wing as it seals is not moved.
    #[test]
    fn a_player_inside_a_wing_at_the_seal_is_not_moved() {
        let mut sim = sanatorium_sim();
        open_every_door(&mut sim);
        sim.world_mut().resource_mut::<Power>().switch_on();
        let inside = Vec3::new(-40.0, 0.0, 17.0);
        put_player(&mut sim, inside);
        let player = the_player(&mut sim);
        show(&mut sim, Stage::Warning, Some(0), 9.9);
        sim.run(sim.frames(1.0));
        assert!(matches!(phase(&mut sim, 0), PiecePhase::Swinging { .. }), "it began to seal");
        sim.run(sim.frames(SWING_SECS + 1.0));
        assert_eq!(phase(&mut sim, 0), PiecePhase::Rest(1));
        assert!(feet_of(&mut sim, player).distance(inside) < 0.01, "{}", feet_of(&mut sim, player));
    }

    /// A Player and a Zombie on a Leaf or in its sweep when it starts to Swing land on the outside
    /// landing, shutting or opening, and a Down Player keeps their Last Stand.
    #[test]
    fn a_player_and_a_zombie_in_a_sweep_land_on_the_outside_landing_and_a_downed_player_keeps_their_last_stand() {
        for stage in [Stage::Warning, Stage::Release] {
            let mut sim = sanatorium_sim();
            open_every_door(&mut sim);
            sim.world_mut().resource_mut::<Power>().switch_on();
            // The doorway of Q1 is at (-40, 14), opening onto the West Corridor, the landing 2.5 m out.
            let spot = Vec3::new(-40.5, 0.0, 13.2);
            let landing = landing(0);
            assert!(MapId::Sanatorium.layout().shifting[0].in_way(spot, 0, 1), "the spot is in the sweep");
            put_player(&mut sim, spot);
            let player = the_player(&mut sim);
            let half = Stance::Standing.half_height();
            let downed = sim.world_mut().spawn((Player, Transform::from_translation(Vec3::new(-39.4, 0.0, 13.0) + Vec3::Y * half), LastStand::shown(10.0, false))).id();
            let zombie = sim.spawn(Vec3::new(-39.6, 0.0, 13.2), None);
            let before = sim.world_mut().get::<Transform>(zombie).unwrap().translation;
            if stage == Stage::Warning {
                // Shutting: the Warning's last moments.
                show(&mut sim, Stage::Warning, Some(0), 9.9);
            } else {
                // Opening: the Wing is Sealed, and its Release ends.
                show_piece(&mut sim, 0, PiecePhase::Rest(1));
                show_piece(&mut sim, 1, PiecePhase::Rest(1));
                show(&mut sim, Stage::Release, Some(0), QUARANTINE_RELEASE_SECS - 0.05);
            }
            sim.run(sim.frames(0.4));
            assert!(matches!(phase(&mut sim, 0), PiecePhase::Swinging { .. }), "{stage:?}: it began to swing");
            for who in [player, downed] {
                let feet = feet_of(&mut sim, who);
                assert!(feet.distance(landing) < 0.3, "{stage:?}: {feet} vs the landing {landing}");
            }
            assert!(sim.world_mut().get::<LastStand>(downed).is_some(), "the Down Player is still in their Last Stand");
            let zombie_at = sim.world_mut().get::<Transform>(zombie).unwrap().translation;
            assert!(zombie_at.distance(before) > 1.0, "the Zombie was moved");
            let zombie_feet = zombie_at - Vec3::Y * sim.world_mut().get::<Zombie>(zombie).unwrap().half_height();
            assert!(Vec2::new(zombie_feet.x - landing.x, zombie_feet.z - landing.z).length() < 1.2, "{stage:?}: the Zombie is at {zombie_feet}, the landing {landing}");
            assert!(sim.world_mut().get::<crate::enemies::zombie::Dying>(zombie).is_none(), "it was not killed");
        }
    }

    /// The Spawn Points of a Sealed Wing are silent while it is empty and live with a Player inside,
    /// while its outside neighbours are not live.
    #[test]
    fn the_spawn_points_of_a_sealed_wing_are_silent_while_empty_and_live_with_a_player_inside() {
        let mut sim = sanatorium_sim();
        open_every_door(&mut sim);
        for id in 0..2u8 {
            show_piece(&mut sim, id, PiecePhase::Rest(1));
        }
        let active = |sim: &mut Sim, zone: ZoneId| {
            let world = sim.world_mut();
            world.query::<&mut PlayerZone>().iter_mut(world).next().unwrap().0 = zone;
            world.run_system_once(update_active_zones).unwrap();
            world.resource::<ActiveZones>().0
        };
        let outside = active(&mut sim, ZoneId::WestCorridor);
        assert!(!outside.contains(ZoneId::LongWard) && !outside.contains(ZoneId::DayRoom), "an empty Sealed Wing is silent");
        assert!(outside.contains(ZoneId::GardenCourt) && outside.contains(ZoneId::EntranceHall), "the rest of the Map is live");
        let refectory = active(&mut sim, ZoneId::Refectory);
        assert!(!refectory.contains(ZoneId::DayRoom), "its other doorway is shut too");
        assert!(refectory.contains(ZoneId::TreatmentRoom), "the East Wing, Open, is live from the Refectory");
        let inside = active(&mut sim, ZoneId::DayRoom);
        assert!(inside.contains(ZoneId::LongWard) && inside.contains(ZoneId::DayRoom), "live with a Player inside, by its archway");
        assert!(!inside.contains(ZoneId::Refectory) && !inside.contains(ZoneId::WestCorridor), "its outside neighbours are not");
        for id in 0..2u8 {
            show_piece(&mut sim, id, PiecePhase::Rest(0));
        }
        let open = active(&mut sim, ZoneId::DayRoom);
        assert!(open.contains(ZoneId::Refectory), "Open, the Wing is joined again");
    }

    /// A Boss Round (Gauntlet) holds the clock at All Clear in a Run, and a cleared one frees it.
    #[test]
    fn a_gauntlet_boss_round_holds_the_clock_at_all_clear_in_a_run() {
        use crate::run::gauntlet::arc::is_boss_round;
        let mut sim = Sim::on(MapId::Sanatorium, 20.0, |app| {
            app.add_plugins(ShiftingPlugin)
                .insert_resource(ActiveZones(Default::default()))
                .init_resource::<Power>()
                .insert_resource(Mode::Gauntlet)
                .insert_resource(Round::in_progress(GAUNTLET_ACT_ROUNDS))
                .add_message::<Purchased>()
                .add_message::<PlayCue>()
                .add_message::<Teleport>();
        });
        assert!(is_boss_round(GAUNTLET_ACT_ROUNDS));
        open_every_door(&mut sim);
        sim.world_mut().resource_mut::<Power>().switch_on();
        sim.run(sim.frames(200.0));
        assert_eq!(sim.world_mut().resource::<Quarantine>().stage(), Stage::AllClear);
        assert!((0..6).all(|id| phase(&mut sim, id) == PiecePhase::Rest(0)));
        sim.world_mut().resource_mut::<Round>().phase = RoundPhase::Intermission(INTERMISSION_SECS);
        sim.run(sim.frames(35.0));
        assert!(sim.world_mut().resource::<Quarantine>().stage().wing_out());
    }

    /// The Developer step moves the Quarantine on at once, Power or no.
    #[test]
    fn the_developer_step_hurries_the_quarantine_on_a_stage() {
        let mut sim = sanatorium_sim();
        open_every_door(&mut sim);
        show(&mut sim, Stage::AllClear, None, 1.0);
        sim.send(StepQuarantine);
        sim.run(2);
        assert_eq!(sim.world_mut().resource::<Quarantine>().stage(), Stage::Warning);
        sim.send(StepQuarantine);
        sim.run(2);
        assert_eq!(sim.world_mut().resource::<Quarantine>().stage(), Stage::Sealing);
        assert!(matches!(phase(&mut sim, 0), PiecePhase::Swinging { from: 0, to: 1, .. }));
    }
}
