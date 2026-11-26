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
//! What a Blaze does to bodies, Dragged Clear and Sabotage are later tickets; here a Lit Blaze is a flat orange quad.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::authority::{Authority, HostSimulation};
use crate::economy::buy::{Buyable, Purchased, Refused};
use crate::enemies::nav::Route;
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
    #[cfg_attr(not(test), allow(dead_code, reason = "what a Blaze does to bodies is the next ticket"))]
    pub fn first(count: usize) -> LitMask {
        LitMask(((1u16 << count.min(MAX_LINES)) - 1) as u8)
    }

    pub fn contains(self, line: FuelLineId) -> bool {
        line.0 < 8 && self.0 & (1 << line.0) != 0
    }

    pub fn with(self, line: FuelLineId) -> LitMask {
        LitMask(self.0 | (1 << line.0))
    }

    #[cfg_attr(not(test), allow(dead_code, reason = "what a Blaze does to bodies is the next ticket"))]
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
#[cfg_attr(not(test), allow(dead_code, reason = "what a Blaze does to bodies is the next ticket"))]
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
    #[cfg_attr(not(test), allow(dead_code, reason = "what a Blaze does to bodies is the next ticket"))]
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
}

impl Default for FuelLines {
    fn default() -> Self {
        Self { count: 0, phases: [LinePhase::Lit; MAX_LINES] }
    }
}

impl FuelLines {
    /// A Map's `count` lines, every one Lit: how every Run starts.
    pub fn start(count: usize) -> Self {
        Self { count: count.min(MAX_LINES) as u8, phases: [LinePhase::Lit; MAX_LINES] }
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
    #[cfg_attr(not(test), allow(dead_code, reason = "what a Blaze does to bodies is the next ticket"))]
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
    /// cleared. (The Sapper's Sabotage of a line, which stays Lit through it, is a later ticket.) Returns whether any line changed phase.
    pub fn advance(&mut self, dt: f32, boss: bool) -> bool {
        let before: Vec<u8> = self.phases[..usize::from(self.count)].iter().map(|phase| phase.kind()).collect();
        for phase in &mut self.phases[..usize::from(self.count)] {
            if boss {
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
                    }
                }
            }
        }
        let after = self.phases[..usize::from(self.count)].iter().map(|phase| phase.kind());
        before.into_iter().ne(after)
    }

    /// What a Valve of `line` offers now.
    pub fn offer(&self, line: FuelLineId, boss: bool) -> ValveOffer {
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
            LinePhase::FlareUp { .. } => LinePhase::Lit,
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
            .add_systems(OnEnter(GameState::Playing), reset)
            .add_systems(
                Update,
                (use_valves, step_nearest, run_lines).chain().in_set(FireClock).in_set(HostSimulation).run_if(world_runs),
            )
            .add_systems(
                Update,
                (run_shown.run_if(not(in_state(Authority::Host))), sync_nav, show_lines).chain().after(FireClock).run_if(world_runs),
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
        let mut sim = Sim::on(MapId::Refinery, 60.0, |app| {
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
}
