//! Fire: the Refinery's Fuel Lines. A Blaze is a rectangle of fire on one floor of one Zone; a Fuel Line is a group of Blazes that
//! light and go out together, with Valves that Shut and relight it. Built the way the Tide is: authored in `Layout.fuel_lines`, read
//! through one function (`blaze_at`), gating nav with one `Gate` (`Gate::Fire`), decided by the Host and told as one reliable
//! `WorldEvent::FuelLine` (and always in the catch-up).
//!
//! Every line starts the Run Lit. A Valve of a Lit line Shuts it for `VALVE_PRICE`; it stays Shut for `SHUT_SECS`, then Flares up for
//! `FLARE_UP_SECS`, then is Lit again. A line Shut for `RELIGHT_AFTER_SECS` can be relit for free from either Valve, straight into a
//! Flare-up; no Valve works during a Flare-up. Through a Gauntlet Boss Round every line is Shut and the clocks hold. Zombies route
//! round a Lit or Flaring line and across a Shut one (`Gate::Fire`, and a straight walk that would cross a closed Blaze is no walk).
//!
//! What a Lit Blaze does to bodies: a Player whose feet are in it takes `BLAZE_PLAYER_DAMAGE` every `BLAZE_PLAYER_TICK_SECS` (through
//! `PlayerHit`, Hit Grace or not, the first tick that long after stepping in) and moves at `BLAZE_SPEED_MULT` (worked out in `step_move`
//! from the `LitMask`, so a client predicts it); a Player who goes Down in one is Dragged Clear, Last Stand kept; a Zombie is set
//! alight with an uncredited Burn of `BLAZE_BURN_SHARE` of its max health per tick (80% in all, Unpaid kills) and slowed
//! (`ZombieMoveFactor`); a Napalm takes no harm; the Sapper and a Boss lose one Trap's share of health per Lit spell that catches them.
//! The Sapper can Sabotage a line: a Shut line flares up and then stays Lit until a Player restores it at a Valve, or the Sapper dies,
//! even through a Boss Round. The Host judges all of it; a client sees the result in the snapshot, the `Teleport` and the Sabotage
//! events, and predicts only its own slow.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::audio::PlayCue;
use crate::audio::cues::Cue;
use crate::authority::{Authority, HostSimulation};
use crate::economy::buy::{Buyable, Purchased, Refused};
use crate::enemies::nav::{NavGraph, Route, zone_at};
use crate::enemies::sapper::Sabotaged;
use crate::enemies::target::{Players, half_height_above_feet};
use crate::enemies::zombie::{Attack, DamageZombie, Dying, Heavyweight, HitZone, Rising, Zombie, burns_in_a_blaze};
use crate::maps::zone::ActiveZones;
use crate::players::last_stand::LastStand;
use crate::players::player::{Locomotion, Player, PlayerHit, Stance, Teleport};
use crate::players::respawn::AwaitingRespawn;
use crate::ui::hud::overlay::{Overlay, Overlays};
use crate::weapons::arsenal::Burn;
use crate::weapons::burn::Burning;
use crate::maps::map::{Layout, MapId};
use crate::maps::zone::ZoneId;
use crate::run::mode::Mode;
use crate::run::round::Round;
use crate::state::{GameState, InRun, world_runs};
use crate::tuning::*;

/// The most Fuel Lines a Map can have: they fit a `u8` mask.
pub const MAX_LINES: usize = 8;

/// A Fuel Line's place in `Layout.fuel_lines`, the same on every machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FuelLineId(pub u8);

/// A set of Fuel Lines, one bit each by `FuelLineId`: those that are closed to Zombies (Lit or Flaring up), or that burn (Lit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LitMask(pub u8);

impl LitMask {
    pub const NONE: LitMask = LitMask(0);

    /// The first `count` lines.
    #[cfg_attr(not(test), allow(dead_code, reason = "the tests light the first few lines"))]
    pub fn first(count: usize) -> LitMask {
        LitMask(((1u16 << count.min(MAX_LINES)) - 1) as u8)
    }

    pub fn contains(self, line: FuelLineId) -> bool {
        line.0 < 8 && self.0 & (1 << line.0) != 0
    }

    pub fn with(self, line: FuelLineId) -> LitMask {
        LitMask(self.0 | (1 << line.0))
    }

    #[cfg_attr(not(test), allow(dead_code, reason = "the tests take a line out of a mask"))]
    pub fn without(self, line: FuelLineId) -> LitMask {
        LitMask(self.0 & !(1 << line.0))
    }
}

/// A patch of fire: a rectangle on a floor of a Zone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blaze {
    pub zone: ZoneId,
    pub min: Vec2,
    pub max: Vec2,
    /// The y of the floor it burns on.
    pub floor: f32,
}

/// A group of Blazes that are Lit and Shut together, and the Valves that do it: each Valve's Zone, where it stands and which way
/// its face looks (x, z).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FuelLine {
    pub name: &'static str,
    pub blazes: &'static [Blaze],
    pub valves: &'static [(ZoneId, Vec3, Vec2)],
}

impl Blaze {
    /// Whether a body with its feet at `feet` is in the rectangle, `margin` metres out from its edges (the edges are in).
    fn holds(&self, feet: Vec3, margin: f32) -> bool {
        (feet.y - self.floor).abs() <= BLAZE_REACH
            && feet.x >= self.min.x - margin
            && feet.x <= self.max.x + margin
            && feet.z >= self.min.y - margin
            && feet.z <= self.max.y + margin
    }

    /// The middle of the rectangle, on its floor.
    pub fn centre(&self) -> Vec3 {
        Vec3::new((self.min.x + self.max.x) / 2.0, self.floor, (self.min.y + self.max.y) / 2.0)
    }
}

/// The Fuel Line whose Blaze (one of those in `lit`) a body with its feet at `feet` stands in, if any. The only question anything
/// asks of a Blaze. The edges of a rectangle are in it.
pub fn blaze_at(layout: &Layout, lit: LitMask, feet: Vec3) -> Option<FuelLineId> {
    layout
        .fuel_lines
        .iter()
        .enumerate()
        .map(|(i, line)| (FuelLineId(i as u8), line))
        .filter(|&(id, _)| lit.contains(id))
        .find(|(_, line)| line.blazes.iter().any(|blaze| blaze.holds(feet, 0.0)))
        .map(|(id, _)| id)
}

/// Whether `at` is clear of every Blaze of the lines in `lit` by `BLAZE_CLEAR_MARGIN`: where a Dragged Clear, a Washed Up or a Respawn may
/// put a body down.
pub fn outside_every_blaze(layout: &Layout, lit: LitMask, at: Vec3) -> bool {
    walk_crosses(layout, lit, at, at, BLAZE_CLEAR_MARGIN).is_none()
}

/// The first closed line (in `lit`) whose Blaze a straight walk from `from` to `to` (feet, the height eased between them) comes within
/// `margin` metres of.
pub fn walk_crosses(layout: &Layout, lit: LitMask, from: Vec3, to: Vec3, margin: f32) -> Option<FuelLineId> {
    if lit == LitMask::NONE || layout.fuel_lines.is_empty() {
        return None;
    }
    let length = from.distance(to);
    let steps = ((length / 0.2).ceil() as usize).max(1);
    (0..=steps).find_map(|i| {
        let feet = from.lerp(to, i as f32 / steps as f32);
        layout
            .fuel_lines
            .iter()
            .enumerate()
            .filter(|&(i, _)| lit.contains(FuelLineId(i as u8)))
            .find(|(_, line)| line.blazes.iter().any(|blaze| blaze.holds(feet, margin)))
            .map(|(i, _)| FuelLineId(i as u8))
    })
}

/// Where a Fuel Line is in its cycle.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LinePhase {
    /// Burning, until a Valve is used.
    Lit,
    /// Put out, `elapsed` seconds into `SHUT_SECS`.
    Shut { elapsed: f32 },
    /// The gas hisses, `elapsed` seconds into `FLARE_UP_SECS`; it burns nothing yet.
    FlareUp { elapsed: f32 },
}

impl LinePhase {
    /// Whether Zombies route round it: Lit, or Flaring up (so they reroute before the fire comes back).
    pub fn closed(self) -> bool {
        !matches!(self, LinePhase::Shut { .. })
    }

    /// Whether it burns.
    pub fn burning(self) -> bool {
        self == LinePhase::Lit
    }

    /// The same phase's kind with no seconds, for telling a change from the seconds running on.
    fn kind(self) -> u8 {
        match self {
            LinePhase::Lit => 0,
            LinePhase::Shut { .. } => 1,
            LinePhase::FlareUp { .. } => 2,
        }
    }
}

/// What a Valve offers a Player now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValveOffer {
    /// Shut the Lit line, for `VALVE_PRICE`.
    Shut,
    /// Relight the line, free.
    Relight,
    /// The line is Shut but not yet for `RELIGHT_AFTER_SECS`.
    Cooling,
    /// The line is Flaring up: no Valve works.
    FlaringUp,
    /// A Boss Round holds every line Shut: no relight.
    Held,
    /// The Sapper has Sabotaged the line: its Valves are dead until a Player restores it.
    Sabotaged,
}

impl ValveOffer {
    /// What the Use key does, in BO1 wording, without the price.
    pub fn prompt(self, line: &str) -> String {
        match self {
            ValveOffer::Shut => format!("Press Use to shut the {line}"),
            ValveOffer::Relight => format!("Press Use to relight the {line}"),
            ValveOffer::Cooling => format!("The {line} is too hot to relight yet"),
            ValveOffer::FlaringUp => format!("The {line} is flaring up"),
            ValveOffer::Held => format!("The {line} is held shut"),
            ValveOffer::Sabotaged => format!("The {line} is sabotaged"),
        }
    }

    /// The price to use it, if it costs.
    pub fn price(self) -> Option<u64> {
        (self == ValveOffer::Shut).then_some(VALVE_PRICE)
    }

    /// Whether the Use key does anything.
    pub fn usable(self) -> bool {
        matches!(self, ValveOffer::Shut | ValveOffer::Relight)
    }
}

/// The Fuel Lines. The Host runs them on world time; every other machine shows what the Host told it and runs the seconds on only to
/// show them.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct FuelLines {
    count: u8,
    phases: [LinePhase; MAX_LINES],
    /// The lines the Sapper has Sabotaged (one bit each), read off the `Sabotaged` marker on each line's `LineNode`, on every machine.
    sabotaged: u8,
    /// How many times each line has lit since the Run began (the Host's count): a body caught in one Lit spell is caught once.
    spells: [u32; MAX_LINES],
}

impl Default for FuelLines {
    fn default() -> Self {
        Self { count: 0, phases: [LinePhase::Lit; MAX_LINES], sabotaged: 0, spells: [0; MAX_LINES] }
    }
}

impl FuelLines {
    /// A Map's `count` lines, every one Lit: how every Run starts.
    pub fn start(count: usize) -> Self {
        Self { count: count.min(MAX_LINES) as u8, ..Self::default() }
    }

    /// The lines the Sapper has Sabotaged.
    pub fn sabotaged(&self) -> LitMask {
        LitMask(self.sabotaged)
    }

    /// Notes which lines are Sabotaged (the Host reads it off the world, a client the same, from the Sabotage it was told).
    pub fn set_sabotaged(&mut self, lines: LitMask) {
        self.sabotaged = lines.0 & (((1u16 << self.count) - 1) as u8);
    }

    /// Which Lit spell `line` is in: it goes up each time the line lights, so a body that a spell has caught once is not caught again by it.
    pub fn spell(&self, line: FuelLineId) -> u32 {
        self.spells.get(usize::from(line.0)).copied().unwrap_or(0)
    }

    pub fn count(&self) -> usize {
        usize::from(self.count)
    }

    pub fn phase(&self, line: FuelLineId) -> LinePhase {
        self.phases.get(usize::from(line.0)).copied().filter(|_| line.0 < self.count).unwrap_or(LinePhase::Lit)
    }

    /// The lines Zombies route round: Lit or Flaring up.
    pub fn closed(&self) -> LitMask {
        self.mask(LinePhase::closed)
    }

    /// The lines that burn: Lit.
    pub fn burning(&self) -> LitMask {
        self.mask(LinePhase::burning)
    }

    fn mask(&self, of: impl Fn(LinePhase) -> bool) -> LitMask {
        (0..self.count).map(FuelLineId).filter(|&line| of(self.phase(line))).fold(LitMask::NONE, LitMask::with)
    }

    /// A client shows the phase the Host says.
    pub fn show(&mut self, line: FuelLineId, phase: LinePhase) {
        if line.0 < self.count {
            self.phases[usize::from(line.0)] = phase;
        }
    }

    /// Runs a client's phases' seconds on, only so they can be shown: nothing leaves its phase until the Host says so.
    pub fn run_shown(&mut self, dt: f32) {
        for phase in &mut self.phases[..usize::from(self.count)] {
            *phase = match *phase {
                LinePhase::Lit => LinePhase::Lit,
                LinePhase::Shut { elapsed } => LinePhase::Shut { elapsed: (elapsed + dt).min(SHUT_SECS) },
                LinePhase::FlareUp { elapsed } => LinePhase::FlareUp { elapsed: (elapsed + dt).min(FLARE_UP_SECS) },
            };
        }
    }

    /// Runs the clocks `dt` seconds on. In a Boss Round every line is Shut and the clocks hold (no Flare-up, no relight) until it is
    /// cleared, except a Sabotaged line, which stays Lit through it. A Sabotaged line that is Shut goes straight into Flare-up, and
    /// once Lit it stays Lit. Returns whether any line changed phase.
    pub fn advance(&mut self, dt: f32, boss: bool) -> bool {
        let before: Vec<u8> = self.phases[..usize::from(self.count)].iter().map(|phase| phase.kind()).collect();
        for i in 0..usize::from(self.count) {
            let sabotaged = self.sabotaged & (1 << i) != 0;
            let phase = &mut self.phases[i];
            if sabotaged && matches!(phase, LinePhase::Shut { .. }) {
                *phase = LinePhase::FlareUp { elapsed: 0.0 };
            }
            if boss && !sabotaged {
                if !matches!(phase, LinePhase::Shut { .. }) {
                    *phase = LinePhase::Shut { elapsed: 0.0 };
                }
                continue;
            }
            let mut left = dt;
            loop {
                match *phase {
                    LinePhase::Lit => break,
                    LinePhase::Shut { elapsed } => {
                        if elapsed + left < SHUT_SECS {
                            *phase = LinePhase::Shut { elapsed: elapsed + left };
                            break;
                        }
                        left -= SHUT_SECS - elapsed;
                        *phase = LinePhase::FlareUp { elapsed: 0.0 };
                    }
                    LinePhase::FlareUp { elapsed } => {
                        if elapsed + left < FLARE_UP_SECS {
                            *phase = LinePhase::FlareUp { elapsed: elapsed + left };
                            break;
                        }
                        left -= FLARE_UP_SECS - elapsed;
                        *phase = LinePhase::Lit;
                        self.spells[i] += 1;
                    }
                }
            }
        }
        let after = self.phases[..usize::from(self.count)].iter().map(|phase| phase.kind());
        before.into_iter().ne(after)
    }

    /// What a Valve of `line` offers now.
    pub fn offer(&self, line: FuelLineId, boss: bool) -> ValveOffer {
        if self.sabotaged().contains(line) {
            return ValveOffer::Sabotaged;
        }
        match self.phase(line) {
            LinePhase::Lit => ValveOffer::Shut,
            LinePhase::FlareUp { .. } => ValveOffer::FlaringUp,
            LinePhase::Shut { .. } if boss => ValveOffer::Held,
            LinePhase::Shut { elapsed } if elapsed >= RELIGHT_AFTER_SECS => ValveOffer::Relight,
            LinePhase::Shut { .. } => ValveOffer::Cooling,
        }
    }

    /// A Valve of `line` is used (the Points were taken by the buy): Shuts a Lit line, or relights one Shut long enough into a
    /// Flare-up. Returns what it did, or `None` if the Valve does nothing now.
    pub fn use_valve(&mut self, line: FuelLineId, boss: bool) -> Option<ValveOffer> {
        let offer = self.offer(line, boss);
        match offer {
            ValveOffer::Shut => self.phases[usize::from(line.0)] = LinePhase::Shut { elapsed: 0.0 },
            ValveOffer::Relight => self.phases[usize::from(line.0)] = LinePhase::FlareUp { elapsed: 0.0 },
            _ => return None,
        }
        Some(offer)
    }

    /// The Developer mode step: a Lit line is Shut, a Shut one flares up and a Flaring one lights, all for free and with no wait.
    pub fn step_now(&mut self, line: FuelLineId) {
        if line.0 >= self.count {
            return;
        }
        self.phases[usize::from(line.0)] = match self.phase(line) {
            LinePhase::Lit => LinePhase::Shut { elapsed: 0.0 },
            LinePhase::Shut { .. } => LinePhase::FlareUp { elapsed: 0.0 },
            LinePhase::FlareUp { .. } => {
                self.spells[usize::from(line.0)] += 1;
                LinePhase::Lit
            }
        };
    }
}

/// The Fuel Line nearest `at`, by the closest of its Blazes' middles and Valves.
pub fn nearest_line(layout: &Layout, at: Vec3) -> Option<FuelLineId> {
    layout
        .fuel_lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let near = line.blazes.iter().map(|blaze| blaze.centre()).chain(line.valves.iter().map(|valve| valve.1)).map(|spot| spot.distance(at)).fold(f32::INFINITY, f32::min);
            (FuelLineId(i as u8), near)
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

/// A Valve in the world: the Buyable wheel on a Fuel Line.
#[derive(Component)]
pub struct Valve {
    pub line: FuelLineId,
    use_point: Vec3,
}

impl Valve {
    /// Where the Valve is used from, and so where a Restore for a Sabotaged line waits.
    pub fn use_point(&self) -> Vec3 {
        self.use_point
    }
}

/// A Fuel Line as one system, for the Sapper to Sabotage and a Player to restore: it stands over the line's first Blaze and is named like
/// every machine, so the Sabotage can say which line. Both Valves of the line are ways to it.
#[derive(Component)]
pub struct LineNode {
    pub line: FuelLineId,
}

/// A Zombie's, Sapper's or Boss's record of the Lit spells that have scorched it, `(line, spell)`: one spell takes one share.
#[derive(Component, Default)]
struct Scorched(Vec<(FuelLineId, u32)>);

/// A Lit Blaze's flat orange quad, until the Look draws it.
#[derive(Component)]
struct BlazeQuad(FuelLineId);

/// A Valve's lamp: red while its line is Lit, green while Shut, amber through a Flare-up.
#[derive(Component)]
struct ValveLamp(FuelLineId);

/// The lamp and quad materials, by phase.
#[derive(Resource, Clone)]
struct FireMaterials {
    lit: Handle<StandardMaterial>,
    flare: Handle<StandardMaterial>,
    shut: Handle<StandardMaterial>,
}

/// Request to Shut or relight the Fuel Line nearest `at` now, free and with no wait. The Host judges it.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct StepFuelLine {
    pub at: Vec3,
}

/// Spawns the Map's Valves and Blaze quads. Called by `spawn_map`.
pub fn spawn_fire(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, layout: &Layout) {
    if layout.fuel_lines.is_empty() {
        return;
    }
    let emissive = |base: Color, glow: LinearRgba| StandardMaterial { base_color: base, emissive: glow, ..default() };
    let look = FireMaterials {
        lit: materials.add(emissive(Color::srgb(0.9, 0.3, 0.05), LinearRgba::rgb(4.0, 1.2, 0.1))),
        flare: materials.add(emissive(Color::srgb(0.9, 0.7, 0.1), LinearRgba::rgb(2.0, 1.5, 0.1))),
        shut: materials.add(emissive(Color::srgb(0.1, 0.5, 0.15), LinearRgba::rgb(0.1, 1.2, 0.2))),
    };
    let iron = materials.add(Color::srgb(0.13, 0.14, 0.15));
    let post = meshes.add(Cuboid::new(0.3, 1.0, 0.3));
    let wheel = meshes.add(Torus::new(0.14, 0.2));
    let lamp = meshes.add(Sphere::new(0.1));
    for (i, line) in layout.fuel_lines.iter().enumerate() {
        let id = FuelLineId(i as u8);
        if let Some(first) = line.blazes.first() {
            commands.spawn((InRun, Name::new(format!("Fuel Line {}", line.name)), LineNode { line: id }, Transform::from_translation(first.centre() + Vec3::Y * 0.5)));
        }
        for blaze in line.blazes {
            let size = blaze.max - blaze.min;
            let at = blaze.centre() + Vec3::Y * 0.03;
            commands.spawn((
                InRun,
                Name::new(format!("Blaze of the {}", line.name)),
                BlazeQuad(id),
                Mesh3d(meshes.add(Cuboid::new(size.x, 0.04, size.y))),
                MeshMaterial3d(look.lit.clone()),
                Transform::from_translation(at),
                Visibility::default(),
            ));
        }
        for &(_, at, facing) in line.valves {
            let normal = Vec3::new(facing.x, 0.0, facing.y);
            let use_point = at + Vec3::Y * 1.1 + normal * 0.3;
            commands
                .spawn((
                    InRun,
                    Name::new(format!("Valve of the {}", line.name)),
                    Valve { line: id, use_point },
                    Mesh3d(post.clone()),
                    MeshMaterial3d(iron.clone()),
                    Transform::from_translation(at + Vec3::Y * 0.5).looking_to(-normal, Vec3::Y),
                    Visibility::default(),
                ))
                .with_children(|parent| {
                    parent.spawn((Mesh3d(wheel.clone()), MeshMaterial3d(iron.clone()), Transform::from_xyz(0.0, 0.7, 0.2).with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))));
                    parent.spawn((ValveLamp(id), Mesh3d(lamp.clone()), MeshMaterial3d(look.lit.clone()), Transform::from_xyz(0.0, 0.62, 0.0)));
                });
        }
    }
    commands.insert_resource(look);
}

/// Every Run starts with every line Lit.
fn reset(mut lines: ResMut<FuelLines>, map: Option<Res<MapId>>) {
    *lines = FuelLines::start(map.map_or(0, |map| map.layout().fuel_lines.len()));
}

/// The Host's clock: runs the lines' phases, held through a Boss Round.
fn run_lines(mut lines: ResMut<FuelLines>, mode: Option<Res<Mode>>, round: Option<Res<Round>>, time: Res<Time>) {
    if lines.count() == 0 {
        return;
    }
    let mut next = *lines;
    next.advance(time.delta_secs(), crate::maps::quarantine::boss_round(mode.as_deref(), round.as_deref()));
    if next != *lines {
        *lines = next;
    }
}

/// A client's clock: runs the seconds of the phases it was told on, to show them.
fn run_shown(mut lines: ResMut<FuelLines>, time: Res<Time>) {
    if lines.count() > 0 {
        lines.run_shown(time.delta_secs());
    }
}

/// Valves own their Buyables: the Shut with its price, the free relight, or why it can't be used.
fn prompt_valves(
    mut commands: Commands,
    valves: Query<(Entity, &Valve, Option<&Buyable>, Has<Refused>)>,
    lines: Res<FuelLines>,
    map: Option<Res<MapId>>,
    mode: Option<Res<Mode>>,
    round: Option<Res<Round>>,
) {
    let Some(map) = map else { return };
    let boss = crate::maps::quarantine::boss_round(mode.as_deref(), round.as_deref());
    for (entity, valve, buyable, refused) in &valves {
        let Some(line) = map.layout().fuel_lines.get(usize::from(valve.line.0)) else { continue };
        let offer = lines.offer(valve.line, boss);
        // A Sabotaged line's Valves are dead: the Restore waiting at each shows its own Prompt.
        if offer == ValveOffer::Sabotaged {
            if buyable.is_some() || refused {
                commands.entity(entity).remove::<(Buyable, Refused)>();
            }
            continue;
        }
        let (text, price) = (offer.prompt(line.name), offer.price());
        if buyable.is_some_and(|buyable| buyable.prompt == text && buyable.price == price) && refused == !offer.usable() {
            continue;
        }
        let mut valve_entity = commands.entity(entity);
        valve_entity.insert(Buyable { use_point: valve.use_point, prompt: text, price, range: USE_RANGE });
        if offer.usable() {
            valve_entity.remove::<Refused>();
        } else {
            valve_entity.insert(Refused);
        }
    }
}

/// A used Valve Shuts its Lit line or relights its Shut one. The Host decides: a client's use arrives as the same `Purchased`.
fn use_valves(
    mut purchases: MessageReader<Purchased>,
    valves: Query<&Valve>,
    mut lines: ResMut<FuelLines>,
    mode: Option<Res<Mode>>,
    round: Option<Res<Round>>,
) {
    let boss = crate::maps::quarantine::boss_round(mode.as_deref(), round.as_deref());
    for &Purchased { buyable, .. } in purchases.read() {
        let Ok(valve) = valves.get(buyable) else { continue };
        let mut next = *lines;
        if next.use_valve(valve.line, boss).is_some() {
            *lines = next;
        }
    }
}

/// A `StepFuelLine` Shuts or relights the line nearest it now.
fn step_nearest(mut asked: MessageReader<StepFuelLine>, mut lines: ResMut<FuelLines>, map: Option<Res<MapId>>) {
    let Some(&StepFuelLine { at }) = asked.read().last() else { return };
    let Some(line) = map.and_then(|map| nearest_line(map.layout(), at)) else { return };
    let mut next = *lines;
    next.step_now(line);
    if next != *lines {
        *lines = next;
    }
}

/// Has every Zombie plan again whenever the lines Zombies route round change.
fn sync_nav(lines: Res<FuelLines>, mut last: Local<LitMask>, mut routes: Query<&mut Route>) {
    let closed = lines.closed();
    if closed != *last {
        *last = closed;
        // A Zombie part-way along an edge finishes it, then plans again; one with no node to go to plans at once.
        for mut route in &mut routes {
            if route.waypoints.is_empty() {
                route.repath_in = 0.0;
            } else {
                route.finishing = true;
            }
        }
    }
}

/// Reads which lines the Sapper has Sabotaged off the `Sabotaged` marker on each `LineNode`, the same on every machine.
fn sync_sabotage(nodes: Query<(&LineNode, Has<Sabotaged>)>, mut lines: ResMut<FuelLines>) {
    let sabotaged = nodes.iter().filter(|(_, sabotaged)| *sabotaged).fold(LitMask::NONE, |mask, (node, _)| mask.with(node.line));
    if sabotaged != lines.sabotaged() {
        lines.set_sabotaged(sabotaged);
    }
}

/// A Player whose feet are in a Lit Blaze takes `BLAZE_PLAYER_DAMAGE` every `BLAZE_PLAYER_TICK_SECS`, the first tick that long after
/// stepping in, as a hit from nowhere: Hit Grace, which holds off a Zombie's melee, doesn't stop it. Stepping out starts the count over.
fn blaze_players(
    players: Players,
    lines: Res<FuelLines>,
    map: Option<Res<MapId>>,
    mut heat: Local<HashMap<Entity, f32>>,
    mut hits: MessageWriter<PlayerHit>,
    time: Res<Time>,
) {
    let lit = lines.burning();
    let Some(layout) = map.map(|map| map.layout()).filter(|_| lit != LitMask::NONE) else {
        heat.clear();
        return;
    };
    let mut inside = HashSet::new();
    for spot in players.iter().filter(|spot| spot.up) {
        if blaze_at(layout, lit, spot.feet).is_none() {
            continue;
        }
        inside.insert(spot.entity);
        let held = heat.entry(spot.entity).or_insert(0.0);
        *held += time.delta_secs();
        while *held >= BLAZE_PLAYER_TICK_SECS {
            *held -= BLAZE_PLAYER_TICK_SECS;
            hits.write(PlayerHit { player: spot.entity, damage: BLAZE_PLAYER_DAMAGE, from: None });
        }
    }
    heat.retain(|player, _| inside.contains(player));
}

/// A Player who is Down with their feet in a Lit Blaze, whether they went Down in it or it lit under them, is moved by the Host's
/// `Teleport` to the nearest path node outside every Lit Blaze in an open Zone (Washed Up's search), their Last Stand carrying on there.
/// So no Player lies Down in the fire and no reviver has to stand in it, and nothing here Downs anyone again.
fn drag_clear(
    mut players: Query<(Entity, &mut Transform, Option<&Locomotion>), (With<Player>, With<LastStand>, Without<AwaitingRespawn>)>,
    lines: Res<FuelLines>,
    map: Option<Res<MapId>>,
    graph: Option<Res<NavGraph>>,
    open: Option<Res<ActiveZones>>,
    mut teleport: MessageWriter<Teleport>,
) {
    let lit = lines.burning();
    let Some(map) = map.filter(|_| lit != LitMask::NONE) else { return };
    let layout = map.layout();
    let Some(graph) = graph.filter(|graph| graph.map == *map) else { return };
    for (player, mut transform, locomotion) in &mut players {
        if locomotion.is_some_and(Locomotion::flying) {
            continue;
        }
        let feet = transform.translation - Vec3::Y * half_height_above_feet(locomotion);
        if blaze_at(layout, lit, feet).is_none() {
            continue;
        }
        let own = zone_at(*map, feet + Vec3::Y * 0.1);
        let Some(to) = crate::players::out_of_bounds::washed_up_target(&graph, layout, Default::default(), feet, own, open.as_deref(), lit) else { continue };
        transform.translation = to + Vec3::Y * (Stance::Standing.half_height() + 0.05);
        teleport.write(Teleport { player, feet: to, facing: None });
    }
}

/// What a Lit Blaze does to the bodies of the Zombies. A Zombie that burns (a Zombie, Hellhound, Shrieker, Nova Crawler or Elite) is set
/// alight, uncredited, with a Burn of `BLAZE_BURN_SHARE` of its max health per tick for `BLAZE_BURN_SECS`, and set alight again once it
/// runs out if it is still inside; its kills are Unpaid. A Napalm takes no harm. The Sapper and a Boss (a `Heavyweight`) aren't set
/// alight: each loses `TRAP_SAPPER_HEALTH_FRACTION` of its max health once per Lit spell that catches it.
fn blaze_zombies(
    mut commands: Commands,
    mut zombies: Query<(Entity, &Zombie, &Transform, Has<Burning>, Option<&mut Scorched>, Has<Heavyweight>), (Without<Dying>, Without<Rising>)>,
    lines: Res<FuelLines>,
    map: Option<Res<MapId>>,
    mut damage: MessageWriter<DamageZombie>,
) {
    let lit = lines.burning();
    let Some(layout) = map.map(|map| map.layout()).filter(|_| lit != LitMask::NONE) else { return };
    for (entity, zombie, transform, alight, scorched, heavyweight) in &mut zombies {
        let feet = transform.translation - Vec3::Y * zombie.half_height();
        let Some(line) = blaze_at(layout, lit, feet) else { continue };
        if heavyweight {
            let spell = (line, lines.spell(line));
            let new = match scorched {
                Some(scorched) if scorched.0.contains(&spell) => false,
                Some(mut scorched) => {
                    scorched.0.push(spell);
                    true
                }
                None => {
                    commands.entity(entity).insert(Scorched(vec![spell]));
                    true
                }
            };
            if new {
                damage.write(DamageZombie {
                    player: None,
                    zombie: entity,
                    damage: zombie.max_health() * TRAP_SAPPER_HEALTH_FRACTION,
                    zone: HitZone::Torso,
                    attack: Attack::Blaze,
                });
            }
        } else if burns_in_a_blaze(zombie, false) && !alight {
            let burn = Burn { damage: zombie.max_health() * BLAZE_BURN_SHARE, tick_secs: BLAZE_BURN_TICK_SECS, secs: BLAZE_BURN_SECS };
            commands.entity(entity).insert(Burning::of_a_blaze(burn));
        }
    }
}

/// How strongly the heat overlay shows with the feet in a Lit Blaze.
const BLAZE_HEAT_STRENGTH: f32 = 0.5;

/// On the Player's own screen: the heat overlay and the burn sound while their feet are in a Lit Blaze, as in a Napalm's pool. Worked out
/// locally from the same `LitMask` and feet, so no message is needed.
fn feel_the_heat(
    players: Players,
    lines: Res<FuelLines>,
    map: Option<Res<MapId>>,
    overlays: Option<ResMut<Overlays>>,
    mut cues: MessageWriter<PlayCue>,
    mut since: Local<f32>,
    time: Res<Time>,
) {
    let lit = lines.burning();
    let in_it = map
        .filter(|_| lit != LitMask::NONE)
        .zip(players.iter().find(|spot| spot.local && spot.up))
        .is_some_and(|(map, spot)| blaze_at(map.layout(), lit, spot.feet).is_some());
    if !in_it {
        *since = 0.0;
        return;
    }
    if let Some(mut overlays) = overlays {
        overlays.hold(Overlay::NapalmHeat, BLAZE_HEAT_STRENGTH);
    }
    *since += time.delta_secs();
    if *since >= BLAZE_PLAYER_TICK_SECS {
        *since -= BLAZE_PLAYER_TICK_SECS;
        cues.write(PlayCue::new(Cue::FireBurn, None));
    }
}

/// Shows each Blaze while its line is Lit or Flaring up, and each Valve's lamp by phase.
fn show_lines(
    lines: Res<FuelLines>,
    look: Option<Res<FireMaterials>>,
    mut quads: Query<(&BlazeQuad, &mut Visibility, &mut MeshMaterial3d<StandardMaterial>), Without<ValveLamp>>,
    mut lamps: Query<(&ValveLamp, &mut MeshMaterial3d<StandardMaterial>), Without<BlazeQuad>>,
) {
    let Some(look) = look else { return };
    let material = |phase: LinePhase| match phase {
        LinePhase::Lit => &look.lit,
        LinePhase::FlareUp { .. } => &look.flare,
        LinePhase::Shut { .. } => &look.shut,
    };
    for (quad, mut visibility, mut handle) in &mut quads {
        let phase = lines.phase(quad.0);
        visibility.set_if_neq(if phase.closed() { Visibility::Inherited } else { Visibility::Hidden });
        let want = material(phase);
        if handle.0 != *want {
            handle.0 = want.clone();
        }
    }
    for (lamp, mut handle) in &mut lamps {
        let want = material(lines.phase(lamp.0));
        if handle.0 != *want {
            handle.0 = want.clone();
        }
    }
}

/// The Host's systems that run the Fuel Lines' clock.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct FireClock;

pub struct FirePlugin;

impl Plugin for FirePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FuelLines>()
            .add_message::<StepFuelLine>()
            .add_message::<Purchased>()
            .add_message::<PlayerHit>()
            .add_message::<Teleport>()
            .add_message::<DamageZombie>()
            .add_message::<PlayCue>()
            .add_systems(OnEnter(GameState::Playing), reset)
            .add_systems(
                Update,
                (sync_sabotage, use_valves, step_nearest, run_lines).chain().in_set(FireClock).in_set(HostSimulation).run_if(world_runs),
            )
            .add_systems(
                Update,
                (blaze_players.before(crate::players::player::take_hits), blaze_zombies, drag_clear.after(crate::players::last_stand::go_down))
                    .after(FireClock)
                    .in_set(HostSimulation)
                    .run_if(world_runs),
            )
            .add_systems(
                Update,
                (
                    (run_shown, sync_sabotage).run_if(not(in_state(Authority::Host))),
                    sync_nav,
                    show_lines,
                    feel_the_heat,
                )
                    .chain()
                    .after(FireClock)
                    .run_if(world_runs),
            )
            .add_systems(Update, prompt_valves.before(crate::economy::buy::focus).run_if(world_runs));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maps::map::MapId;

    fn id(i: u8) -> FuelLineId {
        FuelLineId(i)
    }

    #[test]
    fn fuel_line_timeline_runs_shut_45_flare_up_4_then_lit() {
        assert_eq!((SHUT_SECS, FLARE_UP_SECS), (45.0, 4.0));
        let mut lines = FuelLines::start(2);
        lines.use_valve(id(0), false).unwrap();
        assert_eq!(lines.phase(id(0)), LinePhase::Shut { elapsed: 0.0 });
        assert_eq!(lines.phase(id(1)), LinePhase::Lit, "the other line is untouched");
        for _ in 0..44 {
            lines.advance(1.0, false);
        }
        assert_eq!(lines.phase(id(0)), LinePhase::Shut { elapsed: 44.0 });
        assert!(lines.advance(1.0, false), "45 s on, the phase changes");
        assert_eq!(lines.phase(id(0)), LinePhase::FlareUp { elapsed: 0.0 });
        for _ in 0..3 {
            lines.advance(1.0, false);
        }
        assert_eq!(lines.phase(id(0)), LinePhase::FlareUp { elapsed: 3.0 });
        assert!(lines.advance(1.0, false));
        assert_eq!(lines.phase(id(0)), LinePhase::Lit);
        // One long step runs through every phase.
        lines.use_valve(id(0), false).unwrap();
        lines.advance(46.0, false);
        assert_eq!(lines.phase(id(0)), LinePhase::FlareUp { elapsed: 1.0 });
        lines.advance(100.0, false);
        assert_eq!(lines.phase(id(0)), LinePhase::Lit);
    }

    #[test]
    fn every_fuel_line_starts_the_run_lit() {
        let lines = FuelLines::start(6);
        assert!((0..6).all(|i| lines.phase(id(i)) == LinePhase::Lit));
        assert_eq!(lines.closed(), LitMask::first(6));
        assert_eq!(lines.burning(), LitMask(0b11_1111));
        assert_eq!(MapId::Refinery.layout().fuel_lines.len(), 6);
    }

    #[test]
    fn a_valve_shuts_a_lit_line_for_250() {
        let lines = FuelLines::start(1);
        assert_eq!(VALVE_PRICE, 250);
        assert_eq!(lines.offer(id(0), false), ValveOffer::Shut);
        assert_eq!(lines.offer(id(0), false).price(), Some(250));
        assert_eq!(lines.offer(id(0), false).prompt("Rack Line"), "Press Use to shut the Rack Line");
        let mut lines = lines;
        assert_eq!(lines.use_valve(id(0), false), Some(ValveOffer::Shut));
        assert_eq!((lines.closed(), lines.burning()), (LitMask::NONE, LitMask::NONE));
    }

    #[test]
    fn relight_is_refused_under_10_s_shut() {
        let mut lines = FuelLines::start(1);
        lines.use_valve(id(0), false).unwrap();
        lines.advance(9.9, false);
        assert_eq!(lines.offer(id(0), false), ValveOffer::Cooling);
        assert!(!lines.offer(id(0), false).usable());
        assert_eq!(lines.use_valve(id(0), false), None);
        assert!(matches!(lines.phase(id(0)), LinePhase::Shut { .. }));
    }

    #[test]
    fn relight_is_free_after_10_s_and_goes_to_flare_up() {
        let mut lines = FuelLines::start(1);
        lines.use_valve(id(0), false).unwrap();
        lines.advance(RELIGHT_AFTER_SECS, false);
        let offer = lines.offer(id(0), false);
        assert_eq!((offer, offer.price(), offer.usable()), (ValveOffer::Relight, None, true));
        assert_eq!(offer.prompt("Rack Line"), "Press Use to relight the Rack Line");
        assert_eq!(lines.use_valve(id(0), false), Some(ValveOffer::Relight));
        assert_eq!(lines.phase(id(0)), LinePhase::FlareUp { elapsed: 0.0 });
        assert_eq!(lines.closed(), LitMask(1), "closed to Zombies from the start of the Flare-up");
        assert_eq!(lines.burning(), LitMask::NONE, "but it burns nothing yet");
    }

    #[test]
    fn no_valve_use_during_flare_up() {
        let mut lines = FuelLines::start(1);
        lines.use_valve(id(0), false).unwrap();
        lines.advance(SHUT_SECS, false);
        assert!(matches!(lines.phase(id(0)), LinePhase::FlareUp { .. }));
        assert_eq!(lines.offer(id(0), false), ValveOffer::FlaringUp);
        assert_eq!(lines.use_valve(id(0), false), None);
        assert!(matches!(lines.phase(id(0)), LinePhase::FlareUp { .. }));
    }

    #[test]
    fn blaze_at_is_inclusive_at_the_rect_edges_and_follows_the_mask() {
        let layout = MapId::Refinery.layout();
        // The Bund Line (1): (-42, 38) to (-38, 42) on the ground.
        let all = LitMask::first(6);
        let on = |x: f32, y: f32, z: f32| blaze_at(layout, all, Vec3::new(x, y, z));
        assert_eq!(on(-40.0, 0.0, 40.0), Some(id(1)));
        for (x, z) in [(-42.0, 38.0), (-38.0, 42.0), (-42.0, 42.0), (-38.0, 38.0)] {
            assert_eq!(on(x, 0.0, z), Some(id(1)), "the corner ({x}, {z}) is in");
        }
        for (x, z) in [(-42.01, 40.0), (-37.99, 40.0), (-40.0, 37.99), (-40.0, 42.01)] {
            assert_eq!(on(x, 0.0, z), None, "({x}, {z}) is out");
        }
        // A hop clears it, and the floor of another level is not in it.
        assert_eq!(on(-40.0, BLAZE_REACH + 0.01, 40.0), None);
        assert_eq!(on(-40.0, 4.0, 40.0), None);
        // The Gantry Line (2) burns on the catwalk, not under it.
        assert_eq!(on(-28.0, 4.0, 52.0), Some(id(2)));
        assert_eq!(on(-28.0, 0.0, 52.0), None);
        // It follows the mask.
        assert_eq!(blaze_at(layout, all.without(id(1)), Vec3::new(-40.0, 0.0, 40.0)), None);
        assert_eq!(blaze_at(layout, LitMask::NONE, Vec3::new(-40.0, 0.0, 40.0)), None);
        assert_eq!(blaze_at(layout, LitMask::NONE.with(id(1)), Vec3::new(-40.0, 0.0, 40.0)), Some(id(1)));
        // No Map but the Refinery has a Blaze.
        assert_eq!(blaze_at(MapId::Greybox.layout(), LitMask(0xFF), Vec3::new(0.0, 0.0, 0.0)), None);
    }

    #[test]
    fn a_walk_over_a_closed_blaze_is_no_walk_and_over_a_shut_one_it_is() {
        let layout = MapId::Refinery.layout();
        let (from, to) = (Vec3::new(-46.0, 0.0, 40.0), Vec3::new(-34.0, 0.0, 40.0));
        assert_eq!(walk_crosses(layout, LitMask::first(6), from, to, 0.0), Some(id(1)));
        assert_eq!(walk_crosses(layout, LitMask::first(6).without(id(1)), from, to, 0.0), None);
        let beside = (Vec3::new(-46.0, 0.0, 43.0), Vec3::new(-34.0, 0.0, 43.0));
        assert_eq!(walk_crosses(layout, LitMask::first(6), beside.0, beside.1, 0.0), None);
        assert_eq!(walk_crosses(layout, LitMask::first(6), beside.0, beside.1, 1.5), Some(id(1)), "within the margin");
    }

    #[test]
    fn boss_round_shuts_every_line_and_holds_the_clocks() {
        let mut lines = FuelLines::start(3);
        lines.use_valve(id(1), false).unwrap();
        lines.advance(SHUT_SECS, false);
        assert!(matches!(lines.phase(id(1)), LinePhase::FlareUp { .. }));
        lines.advance(2.0, false);
        // The Round starts: every line goes Shut, the Flare-up too.
        assert!(lines.advance(0.0, true));
        assert!((0..3).all(|i| lines.phase(id(i)) == LinePhase::Shut { elapsed: 0.0 }), "{lines:?}");
        assert_eq!(lines.closed(), LitMask::NONE);
        // The clocks hold: long past 45 s no line flares up.
        lines.advance(200.0, true);
        assert!((0..3).all(|i| lines.phase(id(i)) == LinePhase::Shut { elapsed: 0.0 }));
        // No relight, however long it has been Shut.
        assert_eq!(lines.offer(id(0), true), ValveOffer::Held);
        assert_eq!(lines.use_valve(id(0), true), None);
        // Cleared: the clocks run again from where they held.
        lines.advance(SHUT_SECS - 1.0, false);
        assert!((0..3).all(|i| matches!(lines.phase(id(i)), LinePhase::Shut { .. })));
        lines.advance(1.0, false);
        assert!((0..3).all(|i| matches!(lines.phase(id(i)), LinePhase::FlareUp { .. })));
    }

    #[test]
    fn the_developer_step_shuts_then_flares_then_lights_a_line() {
        let mut lines = FuelLines::start(1);
        lines.step_now(id(0));
        assert!(matches!(lines.phase(id(0)), LinePhase::Shut { .. }));
        lines.step_now(id(0));
        assert!(matches!(lines.phase(id(0)), LinePhase::FlareUp { .. }));
        lines.step_now(id(0));
        assert_eq!(lines.phase(id(0)), LinePhase::Lit);
    }

    #[test]
    fn a_client_runs_the_shown_seconds_on_but_never_out_of_the_phase() {
        let mut lines = FuelLines::start(1);
        lines.show(id(0), LinePhase::Shut { elapsed: 40.0 });
        lines.run_shown(100.0);
        assert_eq!(lines.phase(id(0)), LinePhase::Shut { elapsed: SHUT_SECS });
    }

    #[test]
    fn the_nearest_line_is_by_its_blazes_and_valves() {
        let layout = MapId::Refinery.layout();
        assert_eq!(nearest_line(layout, Vec3::new(-40.0, 0.0, 40.0)), Some(id(1)));
        assert_eq!(nearest_line(layout, Vec3::new(-37.0, 0.0, 12.0)), Some(id(5)));
        assert_eq!(nearest_line(MapId::Greybox.layout(), Vec3::ZERO), None);
    }

    // ---- Headless, on the real Refinery ----

    use crate::economy::buy::{BuyRefused, CantAfford, HoldPurchased, RefusedByPlayer, UseRequest};
    use crate::economy::points::Points;
    use crate::enemies::zombie::headless::Sim;
    use crate::maps::door::Door;
    use crate::maps::zone::{ActiveZones, ZoneSet};
    use crate::players::player::{Player, Stance};

    fn refinery_sim() -> Sim {
        refinery_sim_with(|_| {})
    }

    fn refinery_sim_with(extra: impl FnOnce(&mut App)) -> Sim {
        let mut sim = Sim::on(MapId::Refinery, 60.0, |app| {
            extra(app);
            app.add_plugins(FirePlugin)
                .insert_resource(ActiveZones(Default::default()))
                .add_message::<UseRequest>()
                .add_message::<HoldPurchased>()
                .add_message::<CantAfford>()
                .add_message::<BuyRefused>()
                .add_message::<RefusedByPlayer>()
                .add_systems(Update, crate::economy::buy::resolve_use.in_set(HostSimulation));
        });
        sim.world_mut().resource_mut::<ActiveZones>().0 = ZoneSet::from_iter(crate::maps::zone::ZoneId::ALL);
        let world = sim.world_mut();
        let mut doors = world.query::<&mut Door>();
        for mut door in doors.iter_mut(world) {
            door.open = true;
        }
        sim
    }

    fn move_player(sim: &mut Sim, feet: Vec3) {
        let world = sim.world_mut();
        let mut query = world.query_filtered::<&mut Transform, With<Player>>();
        query.iter_mut(world).next().unwrap().translation = feet + Vec3::Y * Stance::Standing.half_height();
    }

    fn lines(sim: &mut Sim) -> Mut<'_, FuelLines> {
        sim.world_mut().resource_mut::<FuelLines>()
    }

    /// The Player is on the north side of the Bund Line's gap between the tanks, a Zombie on the south. Lit, the gap is no way through
    /// and the Zombie goes round a tank, never setting foot in the Blaze; Shut, it walks straight through the gap, in far less time.
    #[test]
    fn zombies_route_round_a_lit_line_and_across_a_shut_one() {
        let layout = MapId::Refinery.layout();
        let run = |shut: bool| -> (f32, Option<f32>) {
            let mut sim = refinery_sim();
            move_player(&mut sim, Vec3::new(-40.0, 0.0, 50.0));
            sim.run(30);
            if shut {
                lines(&mut sim).use_valve(FuelLineId(1), false).unwrap();
            }
            let zombie = sim.spawn(Vec3::new(-40.0, 0.0, 28.0), None);
            let (mut in_fire, mut arrived) = (0, None);
            for frame in 0..sim.frames(90.0) {
                sim.run(1);
                let Some(at) = sim.world_mut().get::<Transform>(zombie).map(|t| t.translation) else { break };
                let feet = at - Vec3::Y * (ZOMBIE_RADIUS + ZOMBIE_CAPSULE_LENGTH / 2.0);
                if blaze_at(layout, LitMask::first(6), feet).is_some() {
                    in_fire += 1;
                }
                if arrived.is_none() && Vec2::new(at.x + 40.0, at.z - 50.0).length() < 3.0 {
                    arrived = Some(frame as f32 * sim.frame_secs);
                }
            }
            (in_fire as f32, arrived)
        };
        let (through_fire, round) = run(false);
        assert_eq!(through_fire, 0.0, "the Zombie never set foot in a Lit Blaze");
        let round = round.expect("it still got to the Player, the long way round");
        let (_, straight) = run(true);
        let straight = straight.expect("through the Shut gap");
        assert!(straight < round * 0.7, "straight through in {straight} s, round in {round} s");
    }

    /// The gate closes (the line lights) while a Zombie is part-way along the edge through the gap: it finishes the edge to its node, and
    /// then plans again, round the tank, instead of turning in the Blaze.
    #[test]
    fn a_zombie_mid_edge_when_the_gate_closes_finishes_the_edge_and_replans() {
        let mut sim = refinery_sim();
        // The Player is round the corner of Tank B, so the Zombie's way is by nodes, through the gap.
        move_player(&mut sim, Vec3::new(-28.0, 0.0, 56.0));
        sim.run(30);
        lines(&mut sim).use_valve(FuelLineId(1), false).unwrap();
        let zombie = sim.spawn(Vec3::new(-40.0, 0.0, 28.0), None);
        // Walks on until it is in the gap's mouth, south of the Blaze (z 38), with a node to go to.
        let mut mid = None;
        for _ in 0..sim.frames(30.0) {
            sim.run(1);
            let at = sim.world_mut().get::<Transform>(zombie).unwrap().translation;
            let nodes = !sim.world_mut().get::<crate::enemies::nav::Route>(zombie).unwrap().waypoints.is_empty();
            if at.z > 34.5 && nodes {
                mid = Some(at);
                break;
            }
        }
        assert!(mid.is_some(), "it walked into the gap");
        let heading = sim.world_mut().get::<crate::enemies::nav::Route>(zombie).unwrap().waypoints.first().copied().expect("heading for a node");
        // The line lights under it.
        lines(&mut sim).show(FuelLineId(1), LinePhase::Lit);
        sim.run(2);
        let route = sim.world_mut().get::<crate::enemies::nav::Route>(zombie).unwrap();
        assert!(route.finishing, "it was told to finish its edge");
        assert_eq!(route.waypoints.first().copied(), Some(heading), "to the node it was heading for");
        assert_eq!(route.waypoints.len(), 1);
        // It reaches that node, plans again (the flag clears) and goes on to the Player.
        let (mut replanned, mut arrived) = (false, false);
        for _ in 0..sim.frames(60.0) {
            sim.run(1);
            let Some(at) = sim.world_mut().get::<Transform>(zombie).map(|t| t.translation) else { break };
            replanned |= !sim.world_mut().get::<crate::enemies::nav::Route>(zombie).unwrap().finishing;
            arrived |= Vec2::new(at.x + 28.0, at.z - 56.0).length() < 3.0;
        }
        assert!(replanned, "it planned again once it had finished the edge");
        assert!(arrived, "and went on to the Player");
    }

    /// Using a Valve takes its 250 Points from the Player who used it and Shuts its line; no one else pays.
    #[test]
    fn a_valve_use_takes_250_points_and_shuts_the_line() {
        let mut sim = refinery_sim();
        sim.run(10);
        let player = {
            let world = sim.world_mut();
            world.query_filtered::<Entity, With<Player>>().iter(world).next().unwrap()
        };
        sim.world_mut().entity_mut(player).insert(Points { balance: 1000, ..default() });
        let valve = {
            let world = sim.world_mut();
            world.query::<(Entity, &Valve)>().iter(world).find(|(_, valve)| valve.line == FuelLineId(4)).unwrap().0
        };
        let prompt = sim.world_mut().get::<Buyable>(valve).map(|buyable| (buyable.prompt.clone(), buyable.price));
        assert_eq!(prompt, Some(("Press Use to shut the Furnace Line".to_string(), Some(250))));
        sim.send(UseRequest { player, buyable: valve, hold: false });
        sim.run(5);
        assert_eq!(sim.world_mut().get::<Points>(player).unwrap().balance, 750);
        assert!(matches!(lines(&mut sim).phase(FuelLineId(4)), LinePhase::Shut { .. }));
        assert_eq!(lines(&mut sim).closed(), LitMask(0b10_1111), "the other five stay Lit");
        // Shut, its Valve now says why it can't be used for a moment, and charges nothing.
        let after = sim.world_mut().get::<Buyable>(valve).map(|buyable| (buyable.prompt.clone(), buyable.price));
        assert_eq!(after, Some(("The Furnace Line is too hot to relight yet".to_string(), None)));
        sim.send(UseRequest { player, buyable: valve, hold: false });
        sim.run(5);
        assert_eq!(sim.world_mut().get::<Points>(player).unwrap().balance, 750);
    }

    // ---- What a Lit Blaze does to bodies ----

    use crate::enemies::zombie::{ZombieKilled, ZombieMoveFactor, BLAZE};
    use crate::enemies::special::SpecialKind;
    use crate::players::last_stand::LastStand;
    use crate::players::player::HitGrace;
    use crate::run::gauntlet::{Elite, EliteTrait};

    #[derive(Resource, Default)]
    struct Hits(Vec<(f32, f32)>);

    #[derive(Resource, Default)]
    struct Killed(Vec<(Attack, crate::enemies::zombie::Pay)>);

    #[derive(Resource, Default)]
    struct Teleports(Vec<Vec3>);

    fn record(
        mut hits: ResMut<Hits>,
        mut killed: ResMut<Killed>,
        mut teleports: ResMut<Teleports>,
        mut hit: MessageReader<PlayerHit>,
        mut kill: MessageReader<ZombieKilled>,
        mut teleport: MessageReader<Teleport>,
        time: Res<Time>,
    ) {
        hits.0.extend(hit.read().map(|hit| (time.elapsed_secs(), hit.damage)));
        killed.0.extend(kill.read().map(|kill| (kill.attack, kill.pay)));
        teleports.0.extend(teleport.read().map(|teleport| teleport.feet));
    }

    /// The Refinery with the Burn, the kill messages and the recorders, and the Player somewhere harmless.
    fn burning_sim() -> Sim {
        let mut sim = refinery_sim_with(|app| {
            app.add_plugins(crate::weapons::burn::BurnPlugin)
                .add_message::<ZombieKilled>()
                .init_resource::<Hits>()
                .init_resource::<Killed>()
                .init_resource::<Teleports>()
                .add_systems(Update, record);
        });
        move_player(&mut sim, Vec3::new(-40.0, 0.0, 70.0));
        sim
    }

    /// Where a line's first Blaze has its middle, on the floor.
    fn middle_of(line: u8) -> Vec3 {
        MapId::Refinery.layout().fuel_lines[usize::from(line)].blazes[0].centre()
    }

    fn feet_of(sim: &mut Sim, entity: Entity) -> Vec3 {
        let world = sim.world_mut();
        world.get::<Transform>(entity).unwrap().translation - Vec3::Y * world.get::<Zombie>(entity).unwrap().half_height()
    }

    fn secs(sim: &mut Sim) -> f32 {
        sim.world_mut().resource::<Time>().elapsed_secs()
    }

    /// A Player in a Lit Blaze takes 15 every 0.5 s through Hit Grace, the first tick 0.5 s after stepping in; a hop in and out costs
    /// nothing, and standing in a Shut line costs nothing at all.
    #[test]
    fn blaze_player_damage_is_15_per_half_second_with_the_first_tick_after_0_5_s() {
        assert_eq!((BLAZE_PLAYER_DAMAGE, BLAZE_PLAYER_TICK_SECS), (15.0, 0.5));
        let mut sim = burning_sim();
        let player = {
            let world = sim.world_mut();
            world.query_filtered::<Entity, With<Player>>().iter(world).next().unwrap()
        };
        // In Hit Grace: it holds off a Zombie's melee, and fire still hurts.
        assert!(sim.world_mut().entity_mut(player).insert(HitGrace::default()).get_mut::<HitGrace>().unwrap().try_land(std::time::Duration::ZERO));
        let spot = middle_of(1);
        move_player(&mut sim, spot);
        let stepped_in = secs(&mut sim);
        sim.run(sim.frames(2.2));
        let hits = std::mem::take(&mut sim.world_mut().resource_mut::<Hits>().0);
        assert_eq!(hits.len(), 4, "four ticks in 2.2 s: {hits:?}");
        assert!(hits.iter().all(|&(_, damage)| damage == 15.0), "{hits:?}");
        let first = hits[0].0 - stepped_in;
        assert!((0.5 - 0.04..=0.5 + 0.06).contains(&first), "the first tick came {first} s after stepping in");
        for pair in hits.windows(2) {
            assert!((pair[1].0 - pair[0].0 - 0.5).abs() < 0.04, "ticks 0.5 s apart: {hits:?}");
        }
        // A hop in and out costs nothing.
        move_player(&mut sim, spot + Vec3::new(20.0, 0.0, 0.0));
        sim.run(sim.frames(1.0));
        move_player(&mut sim, spot);
        sim.run(sim.frames(0.3));
        move_player(&mut sim, spot + Vec3::new(20.0, 0.0, 0.0));
        sim.run(sim.frames(1.0));
        assert!(sim.world_mut().resource::<Hits>().0.len() <= 1, "a hop of 0.3 s is at most no hit: {:?}", sim.world_mut().resource::<Hits>().0);
        // Shut, it burns nothing.
        lines(&mut sim).use_valve(FuelLineId(1), false).unwrap();
        sim.world_mut().resource_mut::<Hits>().0.clear();
        move_player(&mut sim, spot);
        sim.run(sim.frames(2.0));
        assert!(sim.world_mut().resource::<Hits>().0.is_empty(), "a Shut line is harmless");
        // A Flare-up burns nothing yet either.
        lines(&mut sim).step_now(FuelLineId(1));
        assert!(matches!(lines(&mut sim).phase(FuelLineId(1)), LinePhase::FlareUp { .. }));
        sim.run(sim.frames(1.0));
        assert!(sim.world_mut().resource::<Hits>().0.is_empty(), "a Flare-up is harmless");
    }

    /// Four ticks of a fifth of max health: 80% in all, whatever the health.
    #[test]
    fn blaze_burn_share_totals_80_percent() {
        for max in [100.0_f32, 1000.0, 50_000.0] {
            let burn = Burn { damage: max * BLAZE_BURN_SHARE, tick_secs: BLAZE_BURN_TICK_SECS, secs: BLAZE_BURN_SECS };
            let mut burning = Burning::of_a_blaze(burn);
            let total: f32 = (0..8).map(|_| burning.advance(0.25)).sum();
            assert!((total - max * 0.8).abs() < max * 1e-4, "{max}: {total}");
            assert!(burning.done());
        }
    }

    /// From any Blaze the Dragged Clear's search lands on the nearest path node outside every Lit Blaze, and with fewer lines Lit a
    /// node beside a Shut one is as good.
    #[test]
    fn dragged_clear_lands_on_the_nearest_node_outside_every_lit_blaze() {
        use crate::players::out_of_bounds::washed_up_target;
        let layout = MapId::Refinery.layout();
        let graph = NavGraph::new(MapId::Refinery);
        let all = LitMask::first(6);
        for (i, line) in layout.fuel_lines.iter().enumerate() {
            for blaze in line.blazes {
                let feet = blaze.centre();
                let to = washed_up_target(&graph, layout, Default::default(), feet, None, None, all).expect("somewhere to go");
                assert!(outside_every_blaze(layout, all, to), "line {i}: {to} is in a Blaze");
                let nearest = graph
                    .nodes
                    .iter()
                    .filter(|node| !node.vent && node.zone.is_some() && outside_every_blaze(layout, all, node.at))
                    .map(|node| node.at.distance(feet))
                    .fold(f32::INFINITY, f32::min);
                assert!((to.distance(feet) - nearest).abs() < 0.06, "line {i}: {} m to {to}, the nearest clear node is {nearest} m", to.distance(feet));
                // With only this line Lit, it's no further.
                let alone = washed_up_target(&graph, layout, Default::default(), feet, None, None, LitMask::NONE.with(id(i as u8))).unwrap();
                assert!(alone.distance(feet) <= to.distance(feet) + 0.06);
            }
        }
    }

    /// The Player Down in a Lit Blaze, or in a line that lights under them, is Dragged Clear by the Host's `Teleport` with the Last Stand
    /// kept; Down in a Shut line, they stay put; and a Revive stands them up outside it.
    #[test]
    fn a_down_in_a_blaze_is_dragged_clear_with_the_last_stand_kept() {
        let layout = MapId::Refinery.layout();
        let mut sim = burning_sim();
        let player = {
            let world = sim.world_mut();
            world.query_filtered::<Entity, With<Player>>().iter(world).next().unwrap()
        };
        let feet_of_player = |sim: &mut Sim| {
            let world = sim.world_mut();
            world.get::<Transform>(player).unwrap().translation - Vec3::Y * Stance::Standing.half_height()
        };
        // Down in a Lit Blaze.
        move_player(&mut sim, middle_of(1));
        sim.world_mut().entity_mut(player).insert(LastStand::shown(20.0, true));
        sim.run(5);
        let to = feet_of_player(&mut sim);
        assert!(blaze_at(layout, LitMask::first(6), to).is_none() && outside_every_blaze(layout, LitMask::first(6), to), "Dragged Clear to {to}");
        assert!(sim.world_mut().get::<LastStand>(player).is_some(), "the Last Stand is kept");
        assert_eq!(sim.world_mut().resource::<Teleports>().0.len(), 1, "one Teleport, no loop");
        sim.run(sim.frames(2.0));
        assert_eq!(sim.world_mut().resource::<Teleports>().0.len(), 1, "and nothing more");
        assert!(sim.world_mut().resource::<Hits>().0.is_empty(), "nothing hurts a Player who is Down");
        // The Revive stands them up where they lie: outside every Blaze.
        sim.world_mut().entity_mut(player).remove::<LastStand>();
        sim.run(5);
        assert!(outside_every_blaze(layout, LitMask::first(6), feet_of_player(&mut sim)));
        // Down in a Shut line: left alone, until it lights under them.
        sim.world_mut().resource_mut::<Teleports>().0.clear();
        lines(&mut sim).use_valve(FuelLineId(1), false).unwrap();
        move_player(&mut sim, middle_of(1));
        sim.world_mut().entity_mut(player).insert(LastStand::shown(20.0, true));
        sim.run(sim.frames(1.0));
        assert!(sim.world_mut().resource::<Teleports>().0.is_empty(), "a Shut line doesn't move anyone");
        lines(&mut sim).show(FuelLineId(1), LinePhase::Lit);
        sim.run(5);
        assert_eq!(sim.world_mut().resource::<Teleports>().0.len(), 1, "it lit under them: Dragged Clear");
        assert!(outside_every_blaze(layout, LitMask::first(6), feet_of_player(&mut sim)));
        assert!(sim.world_mut().get::<LastStand>(player).is_some());
    }

    /// No Revive, Respawn or Washed Up puts a body down in a Lit Blaze: a Respawn spot in one moves to the nearest node outside every Lit
    /// Blaze, and the Washed Up search skips any node inside.
    #[test]
    fn a_revive_a_respawn_and_washed_up_never_land_in_a_lit_blaze() {
        use crate::players::respawn::clear_of_fire;
        let layout = MapId::Refinery.layout();
        let graph = NavGraph::new(MapId::Refinery);
        let map = MapId::Refinery;
        let all = FuelLines::start(6);
        let half = PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0;
        for line in 0..6u8 {
            let in_it = middle_of(line) + Vec3::Y * half;
            let spot = clear_of_fire(in_it, Some(&map), Some(&graph), Some(&all));
            let feet = spot - Vec3::Y * half;
            assert!(spot != in_it && outside_every_blaze(layout, all.burning(), feet), "line {line}: a Respawn at {in_it} lands at {spot}");
            // With that line Shut, the same spot is fine.
            let mut shut = all;
            shut.use_valve(id(line), false).unwrap();
            assert_eq!(clear_of_fire(in_it, Some(&map), Some(&graph), Some(&shut)), in_it, "line {line}");
            // Washed Up from the Blaze.
            let washed = crate::players::out_of_bounds::washed_up_target(&graph, layout, Default::default(), middle_of(line), None, None, all.burning()).unwrap();
            assert!(outside_every_blaze(layout, all.burning(), washed), "line {line}: Washed Up to {washed}");
        }
    }

    /// A Zombie caught in a Lit Blaze burns for 80% of its health, is slowed to ×0.6 only while inside, walks out and doesn't stick.
    #[test]
    fn a_zombie_caught_by_a_relight_burns_walks_out_and_doesnt_stick() {
        let layout = MapId::Refinery.layout();
        let mut sim = burning_sim();
        // The line is Shut, a Zombie stands in it, and it relights under them: Flare-up, then Lit.
        lines(&mut sim).use_valve(FuelLineId(1), false).unwrap();
        let spot = middle_of(1);
        let zombie = sim.spawn(spot, None);
        sim.run(sim.frames(1.0));
        assert!(sim.world_mut().get::<Burning>(zombie).is_none(), "a Shut line burns nothing");
        lines(&mut sim).show(FuelLineId(1), LinePhase::Lit);
        sim.run(3);
        assert!(sim.world_mut().get::<Burning>(zombie).is_some(), "set alight");
        let factor = sim.world_mut().get::<ZombieMoveFactor>(zombie).expect("slowed in the Blaze");
        assert!(factor.has(BLAZE) && (factor.factor() - BLAZE_ZOMBIE_SPEED_MULT).abs() < 1e-4);
        let mut left_the_fire_by = None;
        for frame in 0..sim.frames(12.0) {
            sim.run(1);
            let Some(health) = sim.world_mut().get::<Zombie>(zombie).map(Zombie::health) else { break };
            let feet = feet_of(&mut sim, zombie);
            if left_the_fire_by.is_none() && blaze_at(layout, LitMask::first(6), feet).is_none() {
                left_the_fire_by = Some(frame as f32 * sim.frame_secs);
                assert!(health < 1000.0, "it burned on the way out");
            }
        }
        let zombie_gone = sim.world_mut().get::<Zombie>(zombie).is_none() || sim.world_mut().get::<Dying>(zombie).is_some();
        if !zombie_gone {
            let health = sim.world_mut().get::<Zombie>(zombie).unwrap().health();
            assert!(left_the_fire_by.is_some(), "it walked out of the Blaze");
            assert!((health - 200.0).abs() < 1.0, "one Burn is 80% of 1000: {health} left");
            assert!(sim.world_mut().get::<Burning>(zombie).is_none(), "the Burn ran out");
            assert!(sim.world_mut().get::<ZombieMoveFactor>(zombie).is_none_or(|factor| !factor.has(BLAZE)), "the slow went with it");
        }
    }

    /// A kill by a Blaze counts, is Unpaid (and so drops no Power-up), and names no Player.
    #[test]
    fn a_fire_kill_is_unpaid_and_drops_nothing() {
        use crate::enemies::zombie::Pay;
        assert_eq!(Attack::Blaze.pay(), Pay::Unpaid);
        let mut sim = burning_sim();
        let zombie = sim.spawn(middle_of(1), None);
        sim.world_mut().get_mut::<Zombie>(zombie).unwrap().set_health_for_test(30.0);
        sim.run(sim.frames(3.0));
        let kills = sim.world_mut().resource::<Killed>().0.clone();
        assert_eq!(kills, [(Attack::Blaze, Pay::Unpaid)], "{kills:?}");
        // The drop meter only rolls for a Paid kill.
        let mut meter = crate::economy::power_up::DropMeter { armed: true, ..default() };
        assert!(!meter.roll(Attack::Blaze.pay() == Pay::Paid, 0.0, true));
    }

    /// A Napalm walks through unharmed; a Hellhound and an Elite burn like any Zombie.
    #[test]
    fn a_napalm_is_unharmed_and_a_hellhound_and_an_elite_burn() {
        let mut sim = burning_sim();
        let spot = middle_of(1);
        // A Player right beside them, so they stand and fight instead of walking out.
        move_player(&mut sim, spot + Vec3::new(0.0, 0.0, 1.2));
        let napalm = sim.spawn(spot, None);
        sim.world_mut().entity_mut(napalm).insert(Zombie::special(SpecialKind::Napalm, 1000.0, spot));
        let hound = sim.spawn(spot + Vec3::X, None);
        sim.world_mut().entity_mut(hound).insert(Zombie::hellhound(1000.0, spot + Vec3::X));
        let elite = sim.spawn(spot - Vec3::X, None);
        sim.world_mut().entity_mut(elite).insert(Elite(EliteTrait::Volatile));
        sim.run(sim.frames(0.3));
        assert!(sim.world_mut().get::<Burning>(napalm).is_none(), "a Napalm is not set alight");
        assert!(sim.world_mut().get::<ZombieMoveFactor>(napalm).is_none(), "nor slowed");
        assert!(sim.world_mut().get::<Burning>(hound).is_some(), "a Hellhound burns");
        assert!(sim.world_mut().get::<Burning>(elite).is_some(), "an Elite burns");
        sim.run(sim.frames(2.5));
        assert_eq!(sim.world_mut().get::<Zombie>(napalm).unwrap().health(), 1000.0, "a Napalm takes no harm");
        for burned in [hound, elite] {
            let health = sim.world_mut().get::<Zombie>(burned).map_or(0.0, Zombie::health);
            assert!(health <= 200.5, "{burned:?} has {health} left");
        }
    }

    /// The Sapper and a Boss aren't set alight: each loses one Trap's share of its health per Lit spell that catches it, and keeps
    /// its pace; a fresh spell takes another.
    #[test]
    fn the_sapper_and_a_boss_lose_one_trap_share_per_spell() {
        let mut sim = burning_sim();
        let spot = middle_of(1);
        // A Player right beside them, so they stand and fight instead of walking out.
        move_player(&mut sim, spot + Vec3::new(0.0, 0.0, 1.2));
        let sapper = sim.spawn(spot, None);
        sim.world_mut().entity_mut(sapper).insert((Zombie::special(SpecialKind::Sapper, 1000.0, spot), Heavyweight::SAPPER));
        let boss = sim.spawn(spot + Vec3::X, None);
        sim.world_mut().entity_mut(boss).insert(Heavyweight { trap_share: TRAP_SAPPER_HEALTH_FRACTION, ammo_mod_share: 0.05, knife_kill_share: 0.02 });
        let health = |sim: &mut Sim, who| sim.world_mut().get::<Zombie>(who).unwrap().health();
        sim.run(sim.frames(0.5));
        for who in [sapper, boss] {
            assert!(sim.world_mut().get::<Burning>(who).is_none(), "not set alight");
            assert!(sim.world_mut().get::<ZombieMoveFactor>(who).is_none_or(|factor| !factor.has(BLAZE)), "no slow");
            assert!((health(&mut sim, who) - 800.0).abs() < 1.0, "lost one Trap share: {}", health(&mut sim, who));
        }
        // The same spell takes no more, however often they are put back into it.
        let was = (health(&mut sim, sapper), health(&mut sim, boss));
        for _ in 0..3 {
            put_back(&mut sim, [(sapper, spot), (boss, spot + Vec3::X)]);
            sim.run(sim.frames(0.3));
        }
        assert_eq!(was, (health(&mut sim, sapper), health(&mut sim, boss)));
        // A fresh spell: Shut, Flare-up, Lit again, with them put back in.
        let before = lines(&mut sim).spell(FuelLineId(1));
        lines(&mut sim).use_valve(FuelLineId(1), false).unwrap();
        lines(&mut sim).step_now(FuelLineId(1));
        lines(&mut sim).step_now(FuelLineId(1));
        assert_eq!(lines(&mut sim).phase(FuelLineId(1)), LinePhase::Lit);
        assert_eq!(lines(&mut sim).spell(FuelLineId(1)), before + 1);
        put_back(&mut sim, [(sapper, spot), (boss, spot + Vec3::X)]);
        sim.run(sim.frames(0.3));
        for who in [sapper, boss] {
            let feet = feet_of(&mut sim, who);
            assert!((health(&mut sim, who) - 600.0).abs() < 1.0, "a second share: {} for {who:?} at {feet}, scorched {:?}", health(&mut sim, who), sim.world_mut().get::<Scorched>(who).map(|s| s.0.clone()));
        }
    }

    /// Puts each Zombie back on the floor at its spot, a clean cut as a Shove makes one, and has it plan again.
    fn put_back(sim: &mut Sim, who: impl IntoIterator<Item = (Entity, Vec3)>) {
        for (zombie, feet) in who {
            let world = sim.world_mut();
            let at = feet + Vec3::Y * (world.get::<Zombie>(zombie).unwrap().half_height() + 0.02);
            world.get_mut::<Transform>(zombie).unwrap().translation = at;
            world.entity_mut(zombie).insert(avian3d::prelude::Position(at));
            world.get_mut::<bevy_transform_interpolation::TranslationEasingState>(zombie).unwrap().start = Some(at);
            *world.get_mut::<Route>(zombie).unwrap() = Route::default();
        }
    }

    // ---- Sabotage ----

    /// A Sabotaged line goes straight into Flare-up from Shut, stays Lit, refuses its Valves, and stays Lit through a Boss Round; once
    /// restored it is an ordinary line again.
    #[test]
    fn a_sabotaged_line_goes_to_flare_up_stays_lit_and_refuses_its_valves() {
        let mut lines = FuelLines::start(2);
        lines.use_valve(id(0), false).unwrap();
        lines.advance(5.0, false);
        assert!(matches!(lines.phase(id(0)), LinePhase::Shut { .. }));
        lines.set_sabotaged(LitMask::NONE.with(id(0)));
        assert!(lines.advance(0.0, false), "a Shut line goes straight into Flare-up");
        assert_eq!(lines.phase(id(0)), LinePhase::FlareUp { elapsed: 0.0 });
        assert_eq!(lines.closed(), LitMask::first(2));
        assert_eq!(lines.burning(), LitMask::NONE.with(id(1)), "it isn't burning yet");
        lines.advance(FLARE_UP_SECS, false);
        assert_eq!(lines.phase(id(0)), LinePhase::Lit);
        assert_eq!(lines.offer(id(0), false), ValveOffer::Sabotaged);
        assert!(!ValveOffer::Sabotaged.usable());
        assert_eq!(lines.use_valve(id(0), false), None, "its Valves are dead");
        assert_eq!(lines.phase(id(0)), LinePhase::Lit, "and it stays Lit");
        lines.advance(500.0, false);
        assert_eq!(lines.phase(id(0)), LinePhase::Lit);
        // A Boss Round shuts every line but the Sabotaged one.
        lines.advance(0.0, true);
        assert_eq!(lines.phase(id(0)), LinePhase::Lit, "a Sabotaged line stays Lit through a Boss Round");
        assert!(matches!(lines.phase(id(1)), LinePhase::Shut { .. }), "the other is Shut");
        lines.advance(100.0, true);
        assert_eq!(lines.phase(id(0)), LinePhase::Lit);
        // Restored: the Valve works again.
        lines.set_sabotaged(LitMask::NONE);
        assert_eq!(lines.offer(id(0), false), ValveOffer::Shut);
        assert_eq!(lines.use_valve(id(0), false), Some(ValveOffer::Shut));
        // A line that is sabotaged while already Lit just stays Lit.
        let mut lines = FuelLines::start(1);
        lines.set_sabotaged(LitMask::NONE.with(id(0)));
        lines.advance(100.0, false);
        assert_eq!(lines.phase(id(0)), LinePhase::Lit);
    }
}
