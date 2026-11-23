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
use crate::economy::buy::{Buyable, Nearby, Offer, Offers, Purchased, WhileDown};
use crate::enemies::nav::NavGraph;
use crate::enemies::target::half_height_above_feet;
use crate::maps::door::Door;
use crate::maps::map::MapId;
use crate::maps::power::Power;
use crate::maps::shifting::{Driver, PieceId, PiecePhase, PieceState, door_centre, gate_open, outside_landing, pieces};
use crate::players::last_stand::LastStand;
use crate::players::player::{LocalPlayer, Locomotion, Player, Teleport};
use crate::players::respawn::{AwaitingRespawn, Disconnected};
use crate::run::gauntlet::arc::is_boss_round;
use crate::run::mode::Mode;
use crate::run::round::{Round, RoundPhase};
use crate::state::{GameState, InRun, world_runs};
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

/// The Wing of `map` that has `zone` in it, if any.
pub fn wing_of_zone(map: MapId, zone: crate::maps::zone::ZoneId) -> Option<WingId> {
    map.layout().wings.iter().position(|wing| wing.zones.contains(&zone)).map(|wing| WingId(wing as u8))
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

// ---- The Crash Bar ----

/// What the Prompt reads at a Sealed Quarantine Door's inside face.
pub const CRASH_BAR_PROMPT: &str = "Press F to push the Crash Bar";

/// How far above or below the door's floor a Player's feet may be and still be at its Crash Bar.
const CRASH_BAR_FLOOR_SLACK: f32 = 1.5;

/// Where a Quarantine Door's doorway stands, from its Piece and the path nodes either side of it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Doorway {
    /// The middle of the door's plane, on the floor.
    pub centre: Vec3,
    /// Along the floor, out of the Wing: the way its outside face looks (a unit vector).
    pub out: Vec3,
    /// The floor under the two hinges: the ends of the door's face.
    pub ends: [Vec3; 2],
    /// The outside landing: the path node outside the Wing that a Crash Bar puts a Player on.
    pub landing: Vec3,
    /// The path node inside the Wing that its doorway leads to.
    pub inside: Vec3,
}

impl Doorway {
    /// The doorway of the Quarantine Door `id` (a `piece` of `graph`'s Map), if the graph has its
    /// nodes.
    pub fn of(graph: &NavGraph, id: PieceId, piece: &crate::maps::shifting::Piece) -> Option<Self> {
        use crate::enemies::nav::Gate;
        let landing = outside_landing(graph, id, piece)?;
        let rest = graph.edges.iter().find(|edge| edge.gate == Gate::Rest(id, 0))?;
        let inside = graph.nodes[rest.to].at;
        let out = (landing - inside).with_y(0.0).normalize_or_zero();
        let [a, b] = [piece.leaves.first()?.hinge, piece.leaves.last()?.hinge];
        Some(Self { centre: (a + b) / 2.0, out, ends: [a, b], landing, inside })
    }

    /// The nearest point on the door's face to `feet`, at the floor.
    pub fn nearest(&self, feet: Vec3) -> Vec3 {
        let [a, b] = self.ends;
        let along = (b - a).with_y(0.0);
        let t = ((feet - a).with_y(0.0).dot(along) / along.length_squared().max(1e-6)).clamp(0.0, 1.0);
        a + along * t
    }

    /// Whether feet at `feet` stand on the inside of the door, within `CRASH_BAR_RANGE` of its face.
    pub fn inside_reach(&self, feet: Vec3) -> bool {
        (feet - self.centre).with_y(0.0).dot(self.out) <= 0.0
            && (feet - self.nearest(feet)).with_y(0.0).length() <= CRASH_BAR_RANGE
            && (feet.y - self.centre.y).abs() < CRASH_BAR_FLOOR_SLACK
    }
}

/// Whether a Player in this state can push a Crash Bar: up, or in a Last Stand, but not waiting to
/// Respawn or gone from the game.
pub fn can_push(last_stand: bool, awaiting_respawn: bool, disconnected: bool) -> bool {
    crate::enemies::target::is_up(last_stand, awaiting_respawn, disconnected) || (last_stand && !awaiting_respawn && !disconnected)
}

/// Whether a Player standing at `feet` is offered the Crash Bar of the door at `doorway`, which is at
/// `phase`: only inside a Sealed door, within `CRASH_BAR_RANGE` of its face. Never outside, never
/// while the door is Open or Swinging.
pub fn crash_bar_prompts(doorway: &Doorway, phase: PiecePhase, feet: Vec3, can_push: bool) -> bool {
    can_push && phase == PiecePhase::Rest(1) && doorway.inside_reach(feet)
}

/// The push bar inside a Sealed Quarantine Door. A Buyable used, not bought: it asks the Host to put
/// the Player outside.
#[derive(Component, Debug, Clone, Copy)]
pub struct CrashBar {
    pub piece: PieceId,
    pub doorway: Doorway,
}

/// One Crash Bar for each Quarantine Door, on every machine alike.
fn spawn_crash_bars(mut commands: Commands, map: Option<Res<MapId>>, graph: Option<Res<NavGraph>>) {
    let Some(map) = map else { return };
    let layout = map.layout();
    if layout.wings.is_empty() {
        return;
    }
    let own;
    let graph = match graph.as_deref() {
        Some(graph) if graph.map == *map => graph,
        _ => {
            own = NavGraph::new(*map);
            &own
        }
    };
    for (id, piece) in pieces(layout) {
        if piece.wing().is_none() {
            continue;
        }
        let Some(doorway) = Doorway::of(graph, id, piece) else { continue };
        commands.spawn((
            InRun,
            Name::new("Crash Bar"),
            CrashBar { piece: id, doorway },
            Buyable { use_point: doorway.centre + Vec3::Y, prompt: String::new(), price: None, range: -1.0 },
            Nearby,
            WhileDown,
            Offers::default(),
            Transform::from_translation(doorway.centre),
        ));
    }
}

fn phase_of(pieces: &Query<&PieceState>, id: PieceId) -> Option<PiecePhase> {
    pieces.iter().find(|state| state.id() == id).map(PieceState::phase)
}

/// Each Crash Bar offers itself to the `LocalPlayer` only when they could push it: its Prompt and
/// reach are written every frame from what this machine shows.
fn show_crash_bars(
    mut bars: Query<(&CrashBar, &mut Buyable)>,
    pieces: Query<&PieceState>,
    player: Query<(&Transform, Option<&Locomotion>, Has<LastStand>, Has<AwaitingRespawn>, Has<Disconnected>), With<LocalPlayer>>,
) {
    let Ok((transform, locomotion, last_stand, awaiting, disconnected)) = player.single() else { return };
    let feet = transform.translation - Vec3::Y * half_height_above_feet(locomotion);
    let can = can_push(last_stand, awaiting, disconnected);
    for (bar, mut buyable) in &mut bars {
        let offered = phase_of(&pieces, bar.piece).is_some_and(|phase| crash_bar_prompts(&bar.doorway, phase, feet, can));
        let (prompt, range, at) = if offered {
            (CRASH_BAR_PROMPT, CRASH_BAR_RANGE + 2.0, bar.doorway.nearest(feet) + Vec3::Y * (feet.y - bar.doorway.centre.y + 1.0))
        } else {
            ("", -1.0, buyable.use_point)
        };
        if buyable.range != range || buyable.prompt != prompt || buyable.use_point != at {
            buyable.range = range;
            buyable.use_point = at;
            buyable.prompt = prompt.to_string();
        }
    }
}

/// The Host judges each Crash Bar for every Player but the `LocalPlayer` by where they are, for it to
/// hold their request against (see `Offers`).
fn offer_crash_bars(
    mut bars: Query<(&CrashBar, &mut Offers)>,
    pieces: Query<&PieceState>,
    players: Query<(Entity, &Transform, Option<&Locomotion>, Has<LastStand>, Has<AwaitingRespawn>, Has<Disconnected>), (With<Player>, Without<LocalPlayer>)>,
) {
    for (bar, mut offers) in &mut bars {
        let Some(phase) = phase_of(&pieces, bar.piece) else { continue };
        for (player, transform, locomotion, last_stand, awaiting, disconnected) in &players {
            let feet = transform.translation - Vec3::Y * half_height_above_feet(locomotion);
            let judged = crash_bar_prompts(&bar.doorway, phase, feet, can_push(last_stand, awaiting, disconnected))
                .then_some(Offer { price: None, tap: true, hold: None, refused: false });
            if offers.get(player) != Some(judged) {
                offers.set(player, judged);
            }
        }
    }
}

/// A Crash Bar pushed: the Host puts the Player outside the doorway on its landing, with no damage
/// and no Down (a Player in a Last Stand keeps it), and the push sounds on every machine. It judges
/// the Player's place again, whatever the request said.
fn push_crash_bars(
    mut purchases: MessageReader<Purchased>,
    bars: Query<&CrashBar>,
    pieces: Query<&PieceState>,
    mut players: Query<(&mut Transform, Option<&Locomotion>, Has<LastStand>, Has<AwaitingRespawn>, Has<Disconnected>), With<Player>>,
    mut teleport: MessageWriter<Teleport>,
    mut cues: MessageWriter<PlayCue>,
    map: Option<Res<MapId>>,
) {
    let Some(map) = map else { return };
    for &Purchased { player, buyable } in purchases.read() {
        let Ok(bar) = bars.get(buyable) else { continue };
        let Ok((mut transform, locomotion, last_stand, awaiting, disconnected)) = players.get_mut(player) else { continue };
        let half = half_height_above_feet(locomotion);
        let feet = transform.translation - Vec3::Y * half;
        let pushes = phase_of(&pieces, bar.piece).is_some_and(|phase| crash_bar_prompts(&bar.doorway, phase, feet, can_push(last_stand, awaiting, disconnected)));
        if !pushes {
            continue;
        }
        transform.translation = bar.doorway.landing + Vec3::Y * (half + 0.05);
        teleport.write(Teleport { player, feet: bar.doorway.landing, facing: None });
        if let Some(piece) = map.layout().shifting.get(usize::from(bar.piece.0)) {
            cues.write(PlayCue::new(Cue::CrashBar, Some(door_centre(piece))));
        }
    }
}

pub struct QuarantinePlugin;

impl Plugin for QuarantinePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Quarantine>()
            .add_message::<StepQuarantine>()
            .add_message::<PlayCue>()
            .add_plugins(crate::maps::quarantine_look::QuarantineLookPlugin)
            .add_systems(OnEnter(GameState::Playing), (reset, spawn_crash_bars))
            .add_systems(Update, run_clock.in_set(QuarantineClock).in_set(HostSimulation).run_if(world_runs))
            .add_systems(
                Update,
                (run_shown.run_if(not(in_state(Authority::Host))), quarantine_cues).chain().after(QuarantineClock).run_if(world_runs),
            )
            .add_systems(Update, show_crash_bars.before(crate::economy::buy::focus).run_if(world_runs))
            .add_systems(
                Update,
                (offer_crash_bars, push_crash_bars.after(crate::economy::buy::resolve_use)).in_set(HostSimulation).run_if(world_runs),
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

    // ---- The Crash Bar ----

    fn q1() -> Doorway {
        let graph = NavGraph::new(MapId::Sanatorium);
        Doorway::of(&graph, PieceId(0), &MapId::Sanatorium.layout().shifting[0]).unwrap()
    }

    #[test]
    fn crash_bar_prompts_only_inside_a_sealed_door_within_1_2_m() {
        let door = q1();
        assert!((door.out - Vec3::NEG_Z).length() < 1e-3, "Q1 looks out toward -z: {}", door.out);
        let sealed = PiecePhase::Rest(1);
        let face = door.centre;
        for (feet, want) in [
            (face + Vec3::Z * 0.3, true),
            (face + Vec3::Z * 1.1, true),
            (face + Vec3::new(1.0, 0.0, 0.6), true),
            (face + Vec3::Z * 1.3, false),
            (face + Vec3::new(2.5, 0.0, 0.5), false),
            (face + Vec3::Z * 0.5 + Vec3::Y * 4.0, false),
        ] {
            assert_eq!(crash_bar_prompts(&door, sealed, feet, true), want, "{feet}");
        }
    }

    #[test]
    fn crash_bar_prompts_in_a_last_stand() {
        let door = q1();
        let feet = door.centre + Vec3::Z * 0.6;
        assert!(can_push(true, false, false), "a Last Stand can push");
        assert!(crash_bar_prompts(&door, PiecePhase::Rest(1), feet, can_push(true, false, false)));
        assert!(crash_bar_prompts(&door, PiecePhase::Rest(1), feet, can_push(false, false, false)), "up too");
        assert!(!crash_bar_prompts(&door, PiecePhase::Rest(1), feet, can_push(false, true, false)), "not one waiting to Respawn");
        assert!(!can_push(true, false, true), "nor one who has gone");
    }

    #[test]
    fn crash_bar_never_prompts_outside_or_while_open() {
        let door = q1();
        let outside = door.centre + door.out * 0.4;
        assert!(!crash_bar_prompts(&door, PiecePhase::Rest(1), outside, true), "no prompt outside");
        assert!(!crash_bar_prompts(&door, PiecePhase::Rest(1), door.landing, true));
        let inside = door.centre - door.out * 0.6;
        for phase in [PiecePhase::Rest(0), PiecePhase::Swinging { from: 0, to: 1, elapsed: 1.0 }, PiecePhase::Swinging { from: 1, to: 0, elapsed: 1.0 }] {
            assert!(!crash_bar_prompts(&door, phase, inside, true), "{phase:?}");
        }
    }

    fn crash_bar(sim: &mut Sim, piece: u8) -> Entity {
        let world = sim.world_mut();
        world.query::<(Entity, &CrashBar)>().iter(world).find(|(_, bar)| bar.piece == PieceId(piece)).unwrap().0
    }

    fn seal_west_wing(sim: &mut Sim) {
        open_every_door(sim);
        for id in 0..2u8 {
            show_piece(sim, id, PiecePhase::Rest(1));
        }
        show(sim, Stage::Sealed, Some(0), 1.0);
    }

    /// A Crash Bar puts a Player inside a Sealed door outside, on the landing, and from a Last Stand
    /// too (which they keep). Outside, or with the door Open, it does nothing.
    #[test]
    fn a_crash_bar_puts_a_player_outside_and_from_a_last_stand_too() {
        for last_stand in [false, true] {
            let mut sim = sanatorium_sim();
            seal_west_wing(&mut sim);
            let inside = Vec3::new(-40.0, 0.0, 14.6);
            put_player(&mut sim, inside);
            let player = the_player(&mut sim);
            if last_stand {
                sim.world_mut().entity_mut(player).insert(LastStand::shown(10.0, false));
            }
            sim.run(3);
            let bar = crash_bar(&mut sim, 0);
            sim.send(Purchased { player, buyable: bar });
            let mut cues = Vec::new();
            for _ in 0..sim.frames(0.5) {
                sim.run(1);
                cues.extend(sim.world_mut().resource_mut::<Messages<PlayCue>>().drain().map(|cue| cue.cue));
            }
            let feet = feet_of(&mut sim, player);
            assert!(feet.distance(landing(0)) < 0.5, "last_stand {last_stand}: {feet}");
            assert_eq!(sim.world_mut().get::<LastStand>(player).is_some(), last_stand, "the Last Stand is kept");
            assert!(cues.contains(&Cue::CrashBar), "{cues:?}");
        }
        // Standing outside, or with the Wing Open: a request moves no one.
        for sealed in [false, true] {
            let mut sim = sanatorium_sim();
            open_every_door(&mut sim);
            if sealed {
                seal_west_wing(&mut sim);
            }
            let at = if sealed { Vec3::new(-40.0, 0.0, 11.0) } else { Vec3::new(-40.0, 0.0, 14.6) };
            put_player(&mut sim, at);
            let player = the_player(&mut sim);
            sim.run(3);
            let bar = crash_bar(&mut sim, 0);
            sim.send(Purchased { player, buyable: bar });
            sim.run(sim.frames(0.3));
            assert!(feet_of(&mut sim, player).distance(at) < 0.2, "sealed {sealed}");
        }
    }

    // ---- Stranded Zombies ----

    fn stranded_sim() -> Sim {
        use crate::run::round::requeue_cleaned_up;
        Sim::on(MapId::Sanatorium, 20.0, |app| {
            app.add_plugins(ShiftingPlugin)
                .insert_resource(ActiveZones(Default::default()))
                .insert_resource(Round::in_progress(3))
                .init_resource::<Power>()
                .add_message::<Purchased>()
                .add_message::<PlayCue>()
                .add_message::<Teleport>()
                .add_systems(Update, requeue_cleaned_up);
        })
    }

    fn zombie_alive(sim: &mut Sim, zombie: Entity) -> bool {
        sim.world_mut().get_entity(zombie).is_ok()
    }

    /// A Zombie cut off by a seal is Stranded after 5 s, walks to the door and pounds, and is back in the
    /// Horde afterwards, so the Round still ends; the outside Horde drains back the same way.
    #[test]
    fn a_zombie_cut_off_by_a_seal_is_stranded_pounds_and_is_back_in_the_horde() {
        use crate::enemies::zombie::stranded::Stranded;
        let mut sim = stranded_sim();
        seal_west_wing(&mut sim);
        put_player(&mut sim, Vec3::new(-40.0, 0.0, 28.0));
        let outside = [Vec3::new(-34.0, 0.0, 11.0), Vec3::new(-20.0, 0.0, 49.0), Vec3::new(0.0, 0.0, 22.0)];
        let zombies: Vec<Entity> = outside
            .iter()
            .map(|&at| {
                assert!(sim.world_mut().resource_mut::<Round>().take_from_horde());
                sim.spawn(at, None)
            })
            .collect();
        let spawned = sim.world_mut().resource::<Round>().horde_spawned();
        sim.run(sim.frames(3.0));
        assert!(zombies.iter().all(|&z| sim.world_mut().get::<Stranded>(z).is_none()), "not yet stranded at 3 s");
        let (mut stranded_seen, mut pounded, mut sounded) = (false, false, false);
        for _ in 0..sim.frames(40.0) {
            sim.run(1);
            let first = zombies[0];
            if zombie_alive(&mut sim, first) {
                let is_stranded = sim.world_mut().get::<Stranded>(first).is_some();
                stranded_seen |= is_stranded;
                pounded |= sim.world_mut().get::<Stranded>(first).is_some_and(Stranded::pounding);
            }
            sounded |= sim.world_mut().resource_mut::<Messages<PlayCue>>().drain().any(|cue| cue.cue == Cue::DoorPound);
        }
        assert!(stranded_seen, "it was Stranded");
        assert!(pounded, "it swung at the door");
        assert!(sounded, "the blows sounded");
        for &zombie in &zombies {
            assert!(!zombie_alive(&mut sim, zombie), "back in the Horde");
        }
        assert!(sim.world_mut().resource::<Messages<crate::enemies::zombie::ZombieKilled>>().is_empty(), "no kills");
        assert_eq!(sim.world_mut().resource::<Round>().horde_spawned(), spawned - 3, "all three are back in the Horde");
        // They come in again through the Wing's windows: with the Player inside, the only live Window
        // spawn points are the Wing's.
        let zones = {
            let world = sim.world_mut();
            world.query::<&mut PlayerZone>().iter_mut(world).next().unwrap().0 = ZoneId::LongWard;
            world.run_system_once(update_active_zones).unwrap();
            world.resource::<ActiveZones>().0
        };
        let live: Vec<ZoneId> = {
            let world = sim.world_mut();
            world
                .query::<&crate::maps::map::SpawnPoint>()
                .iter(world)
                .filter(|point| matches!(point.kind, crate::maps::map::SpawnKind::Window(_)) && zones.contains(point.zone))
                .map(|point| point.zone)
                .collect()
        };
        assert!(!live.is_empty() && live.iter().all(|zone| matches!(zone, ZoneId::LongWard | ZoneId::DayRoom)), "{live:?}");
    }

    /// A Zombie that can still reach a Player is never Stranded, however long a Wing is Sealed.
    #[test]
    fn a_zombie_that_can_reach_a_player_is_never_stranded_in_a_run() {
        use crate::enemies::zombie::stranded::Stranded;
        let mut sim = stranded_sim();
        seal_west_wing(&mut sim);
        put_player(&mut sim, Vec3::new(-20.0, 0.0, 30.0));
        let zombie = sim.spawn(Vec3::new(-34.0, 0.0, 11.0), None);
        for _ in 0..sim.frames(12.0) {
            sim.run(1);
            assert!(sim.world_mut().get::<Stranded>(zombie).is_none());
        }
    }

    /// A special and a Sapper cut off by a seal are moved to a fresh Special Spawn Point, and stay.
    #[test]
    fn a_stranded_special_and_a_stranded_sapper_are_moved_to_a_fresh_spawn_point() {
        use crate::enemies::special::SpecialKind;
        use crate::run::round::CarriesOver;
        for kind in [SpecialKind::Shrieker, SpecialKind::Sapper] {
            let mut sim = stranded_sim();
            seal_west_wing(&mut sim);
            sim.world_mut().resource_mut::<ActiveZones>().0 = [ZoneId::Forecourt, ZoneId::GardenCourt, ZoneId::Refectory, ZoneId::SunRoof, ZoneId::WestCorridor].into_iter().collect();
            put_player(&mut sim, Vec3::new(-40.0, 0.0, 28.0));
            let start = Vec3::new(-34.0, 0.0, 11.0);
            let zombie = sim.spawn(start, None);
            sim.world_mut().entity_mut(zombie).insert((Zombie::special(kind, 1000.0, start), CarriesOver));
            let world = sim.world_mut();
            let points: Vec<Vec3> = world
                .query::<(&Transform, &crate::maps::map::SpawnPoint)>()
                .iter(world)
                .filter(|(_, p)| p.kind == crate::maps::map::SpawnKind::Special)
                .map(|(t, _)| t.translation)
                .collect();
            let mut last = start;
            let mut jumped = None;
            for _ in 0..sim.frames(8.0) {
                sim.run(1);
                assert!(zombie_alive(&mut sim, zombie), "{kind:?} stays in play");
                let at = sim.world_mut().get::<Transform>(zombie).unwrap().translation;
                if at.distance(last) > 3.0 {
                    jumped = Some(at);
                }
                last = at;
            }
            let at = jumped.unwrap_or_else(|| panic!("{kind:?} was never moved"));
            assert!(points.iter().any(|point| Vec2::new(point.x - at.x, point.z - at.z).length() < 1.5), "{kind:?} at {at}, not on any of {points:?}");
        }
    }
}
