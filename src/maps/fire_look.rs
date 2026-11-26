//! How the Refinery's Fuel Lines look (#572, from #510 section 5 and #507's warning cues): the Blazes, the feed pipes with their
//! burners and the Valves, in every phase. Nothing is on the wire. Every machine poses the same things from `FuelLines` (the phase
//! and `elapsed` the Host told it, and the Sabotage it reads off each line's `LineNode`) and the clock, so a client and a late joiner
//! draw what the Host does.
//!
//! The look is two pure functions and a handful of systems that follow them:
//!
//! - `blaze_pose(phase)` is a Blaze's flames, light, bed glow, smoke, blue pilot and heat shimmer, each 0 to 1 (and `Smoke`, a card
//!   count and an alpha), by phase: Lit, the 1 s shrink into Shut, Shut, each second of the Flare-up, and (with `ignite_ramp`, which
//!   the look times itself because `Lit` carries no seconds) the 0.4 s regrowth into Lit.
//! - `valve_pose(phase, sabotaged, now)` is a Valve's lamp, gauge needle and wheel.
//!
//! A Blaze is a bed (a 0.03 m `burned_ground_01` slab a margin round its rectangle, a `Decal::HazardStripe` kerb on its edge and a
//! `scorch_01` quad that swaps between two shared materials, glow and scorch), a point light (its entity is kept, its intensity is
//! set), and cards of the Kenney sprites the blasts use: flames (one per 2.5 m^2, at most 8 a Blaze, alternating `fire_01` and
//! `fire_02` every 0.12 s), smoke, the blue pilot of the Flare-up and the heat shimmer. The feed pipe runs from Valve to Valve over
//! the Blazes with a burner nozzle every metre along each, and the flames stand on them. The Furnace Blaze, under the Column
//! Platform's grating, lights from 1.5 m up and has a glow strip on the grating's underside while Lit.
//!
//! A Zombie whose feet are in a Lit Blaze, and for 2 s after, wears two flame cards (chest, legs): the same feet rule on every
//! machine (`blaze_at` on the `Transform` each already has), at most 12 drawn, the nearest to the eye first.
//!
//! A Valve is the `valve_stand` kitbash (a riser to 1.1 m, a gauge with a separate needle, a hand wheel and a 2.4 m hazard-striped
//! mast) with the Valve lamp on top, which the game draws as it changes colour. The kitbash glTFs are used when they are on disk and
//! the parts are drawn from code when they are not, so headless runs need no asset files. With no sprites on disk the cards are left
//! out and the rest still builds.

use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use super::fire::{FireClock, FireShown, FuelLine, FuelLineId, FuelLines, LinePhase, blaze_at};
use super::map::MapId;
use super::map::decals::{Decal, DecalPiece};
use super::map::textures::{Material, Shape, Textured, box_uv_mesh, on_disk};
use super::zone::ZoneId;
use crate::enemies::zombie::{Dying, Zombie, burns_in_a_blaze};
use crate::maps::darkness::lamp_flicker::position_hash;
use crate::state::{InRun, world_runs};
use crate::weapons::blast::{BlastBit, envelope, spawn as spawn_blast, Shape as BlastShape};
use crate::weapons::explosive::{BlastKind, ExplosiveAssets};
use crate::weapons::muzzle::sprite_file;

// ---- Measures ----

/// A flame card for each this many square metres of a Blaze, and the most a Blaze has.
pub const FLAME_AREA: f32 = 2.5;
pub const MAX_FLAMES: usize = 8;
/// How tall a flame card stands (m), and how fast it alternates `fire_01` and `fire_02` (s).
pub const FLAME_HEIGHT: (f32, f32) = (1.2, 1.8);
pub const FLAME_SWAP_SECS: f32 = 0.12;
/// A Blaze's flames shrink away over this long on Shutting, and its light fades with them.
pub const SHUT_FADE_SECS: f32 = 1.0;
/// Its bed glow fades to scorch over this long.
pub const GLOW_FADE_SECS: f32 = 2.0;
/// A line grows its flames back over this long on lighting.
pub const IGNITE_SECS: f32 = 0.4;
/// The blue pilot burns for the last this long of a Flare-up.
pub const PILOT_SECS: f32 = 1.5;
/// A Flare-up's light and bed glow come up to these.
pub const FLARE_LIGHT: f32 = 0.15;
pub const FLARE_GLOW: f32 = 0.3;
/// How many rising smoke cards a Lit Blaze has, and how long each takes to rise.
pub const SMOKE_CARDS: usize = 3;
const SMOKE_SECS: f32 = 4.0;
/// How many cards of heat shimmer a Flare-up has a Blaze.
pub const SHIMMER_CARDS: usize = 3;
/// A Blaze's light: colour, strength and reach. Shadowless.
pub const BLAZE_COLOUR: Color = Color::srgb(1.0, 0.55, 0.2);
pub const BLAZE_LUMENS: f32 = 300_000.0;
pub const BLAZE_RANGE: f32 = 9.0;
/// How high a Blaze's light stands: the Furnace Blaze's, under a grating, is higher to light it.
const LIGHT_HEIGHT: f32 = 0.8;
const FURNACE_LIGHT_HEIGHT: f32 = 1.5;
/// How far the bed goes out from a Blaze's rectangle, how thick it is, and how wide the hazard kerb on its edge is.
pub const BED_MARGIN: f32 = 0.6;
pub const BED_THICKNESS: f32 = 0.03;
pub const KERB_WIDTH: f32 = 0.3;
/// How long a kerb tile is, about.
const KERB_TILE: f32 = 0.9;
/// The feed pipe: its radius, and the gap between its burner nozzles.
pub const PIPE_RADIUS: f32 = 0.15;
pub const NOZZLE_GAP: f32 = 1.0;
const NOZZLE_HEIGHT: f32 = 0.25;
/// How many burning Zombies are drawn at once, and how long a Zombie burns for after it leaves a Blaze.
pub const MAX_BURNING_ZOMBIES: usize = 12;
pub const BURN_AFTER_SECS: f32 = 2.0;
/// The Furnace Blaze's glow strip sits this far under the grating's top.
const GRATING_UNDERSIDE: f32 = 0.21;

/// The Valve stand's frame (`tools/props/kitbash.py` repeats these): its foot's middle the origin, +z towards the approach.
pub const RISER: f32 = 1.1;
pub const HUB_AT: Vec3 = Vec3::new(0.0, 1.1, 0.16);
pub const GAUGE_AT: Vec3 = Vec3::new(-0.34, 1.0, 0.06);
pub const MAST_AT: Vec2 = Vec2::new(0.4, -0.12);
pub const MAST_HEIGHT: f32 = 2.4;
/// The wheel's rim (m), and how far the needle's pivot is proud of the gauge case.
pub const WHEEL_RADIUS: f32 = 0.3;
const NEEDLE_PROUD: f32 = 0.034;
/// Where the Valve lamp is over the stand's foot.
pub const LAMP_AT: Vec3 = Vec3::new(MAST_AT.x, MAST_HEIGHT + 0.16, MAST_AT.y);
/// The Valve lamp's emissive strength.
const LAMP_GLOW: f32 = 8.0;
/// The Valve wheel's turn on Shutting: its revolutions, and how long they take.
pub const WHEEL_REVS: f32 = 1.5;
pub const WHEEL_SECS: f32 = 0.8;
/// The lamp blinks, in Hz: amber through a Flare-up, red when Sabotaged.
pub const FLARE_BLINK_HZ: f32 = 3.0;
pub const SABOTAGE_BLINK_HZ: f32 = 6.0;
/// The needle on a Lit line, as a fraction of its sweep.
pub const LIT_NEEDLE: f32 = 0.7;

pub const STAND_MODEL: &str = "props/valve_stand/valve_stand.gltf";
pub const WHEEL_MODEL: &str = "props/valve_wheel/valve_wheel.gltf";
pub const NEEDLE_MODEL: &str = "props/valve_needle/valve_needle.gltf";

const FIRE_SPRITES: [&str; 2] = ["fire_01", "fire_02"];
const SMOKE_SPRITES: [&str; 3] = ["smoke_02", "smoke_03", "smoke_04"];
const SCORCH_SPRITE: &str = "scorch_01";

// ---- Pure poses ----

/// A Blaze's smoke: how many rising cards, and how dark.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Smoke {
    pub cards: usize,
    pub alpha: f32,
}

/// How a Blaze looks in a phase. Every field but `smoke` runs from 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlazePose {
    /// How big the flame cards are, of full.
    pub flames: f32,
    /// How bright the point light is, of full.
    pub light: f32,
    /// How much of the bed glows, of full: 0.5 and over draws the glow material, under it the scorch.
    pub glow: f32,
    pub smoke: Smoke,
    /// The blue pilot flames at the nozzles.
    pub pilot: f32,
    /// The heat shimmer along the pipe.
    pub shimmer: f32,
}

/// The look of a Blaze, from its line's phase alone. `Lit` has no seconds, so the 0.4 s regrowth into it is `ignite_ramp`.
pub fn blaze_pose(phase: LinePhase) -> BlazePose {
    const WISP: Smoke = Smoke { cards: 1, alpha: 0.25 };
    match phase {
        LinePhase::Lit => BlazePose { flames: 1.0, light: 1.0, glow: 1.0, smoke: Smoke { cards: SMOKE_CARDS, alpha: 0.6 }, pilot: 0.0, shimmer: 0.0 },
        LinePhase::Shut { elapsed } => {
            let out = (1.0 - elapsed / SHUT_FADE_SECS).clamp(0.0, 1.0);
            BlazePose {
                flames: out,
                light: out,
                glow: (1.0 - elapsed / GLOW_FADE_SECS).clamp(0.0, 1.0),
                // The three cards thin to the one wisp over the same second the flames shrink.
                smoke: if elapsed < SHUT_FADE_SECS { Smoke { cards: SMOKE_CARDS, alpha: 0.25 + 0.35 * out } } else { WISP },
                pilot: 0.0,
                shimmer: 0.0,
            }
        }
        LinePhase::FlareUp { elapsed } => {
            let along = (elapsed / crate::tuning::FLARE_UP_SECS).clamp(0.0, 1.0);
            BlazePose {
                flames: 0.0,
                light: FLARE_LIGHT * along,
                glow: FLARE_GLOW * along,
                smoke: WISP,
                pilot: if elapsed >= crate::tuning::FLARE_UP_SECS - PILOT_SECS { 1.0 } else { 0.0 },
                shimmer: 1.0,
            }
        }
    }
}

/// How far the flames have regrown, `since` seconds after the line lit.
pub fn ignite_ramp(since: f32) -> f32 {
    (since / IGNITE_SECS).clamp(0.0, 1.0)
}

/// What the Valve lamp shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lamp {
    /// Steady red-orange: the line is Lit.
    RedOrange,
    /// Steady green: Shut.
    Green,
    /// Amber, lit: the on half of its 3 Hz blink through a Flare-up.
    Amber,
    /// Red, lit: the on half of the Sabotage's 6 Hz blink.
    Alarm,
    /// Between blinks.
    Dark,
}

impl Lamp {
    pub const ALL: [Lamp; 5] = [Lamp::RedOrange, Lamp::Green, Lamp::Amber, Lamp::Alarm, Lamp::Dark];

    /// The base colour and the emissive colour of the lens, before the glow strength.
    fn colours(self) -> (Color, LinearRgba) {
        let (base, glow) = match self {
            Lamp::RedOrange => (Color::srgb(0.9, 0.25, 0.05), (1.0, 0.22, 0.04)),
            Lamp::Green => (Color::srgb(0.1, 0.6, 0.15), (0.1, 1.0, 0.2)),
            Lamp::Amber => (Color::srgb(0.9, 0.65, 0.05), (1.0, 0.65, 0.04)),
            Lamp::Alarm => (Color::srgb(0.9, 0.04, 0.03), (1.0, 0.03, 0.02)),
            Lamp::Dark => return (Color::srgb(0.08, 0.07, 0.06), LinearRgba::BLACK),
        };
        (base, LinearRgba::rgb(glow.0 * LAMP_GLOW, glow.1 * LAMP_GLOW, glow.2 * LAMP_GLOW))
    }
}

/// How a Valve looks in a phase.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValvePose {
    pub lamp: Lamp,
    /// The gauge needle, 0 (resting, lower left) to 1 (the red, lower right).
    pub needle: f32,
    /// How far the wheel has turned, in revolutions: 0 while Lit, `WHEEL_REVS` once Shut. The rim is four-fold, so the snap back is not seen.
    pub wheel: f32,
}

/// Whether a blink of `hz` is on at `now`: on for the first half of each period.
pub fn blink(hz: f32, now: f32) -> bool {
    (now * hz).rem_euclid(1.0) < 0.5
}

/// The shake in the needle through a Flare-up, plus or minus 0.04 of its sweep, from the clock alone.
fn needle_jitter(now: f32) -> f32 {
    0.04 * 0.5 * ((now * 23.0).sin() + (now * 37.0 + 1.3).sin())
}

/// The look of a Valve of a line in `phase`, which the Sapper has Sabotaged or not, at `now` seconds.
pub fn valve_pose(phase: LinePhase, sabotaged: bool, now: f32) -> ValvePose {
    let wheel = match phase {
        LinePhase::Lit => 0.0,
        LinePhase::Shut { elapsed } => WHEEL_REVS * (elapsed / WHEEL_SECS).clamp(0.0, 1.0),
        LinePhase::FlareUp { .. } => WHEEL_REVS,
    };
    if sabotaged {
        // Red and fast, the gauge pinned at the red.
        return ValvePose { lamp: if blink(SABOTAGE_BLINK_HZ, now) { Lamp::Alarm } else { Lamp::Dark }, needle: 1.0, wheel };
    }
    match phase {
        LinePhase::Lit => ValvePose { lamp: Lamp::RedOrange, needle: LIT_NEEDLE, wheel },
        LinePhase::Shut { .. } => ValvePose { lamp: Lamp::Green, needle: 0.0, wheel },
        LinePhase::FlareUp { elapsed } => ValvePose {
            lamp: if blink(FLARE_BLINK_HZ, now) { Lamp::Amber } else { Lamp::Dark },
            needle: (elapsed / crate::tuning::FLARE_UP_SECS + needle_jitter(now)).clamp(0.0, 1.0),
            wheel,
        },
    }
}

/// How many flame cards a Blaze of `area` square metres has: one for each 2.5, at most 8, at least 1.
pub fn flame_count(area: f32) -> usize {
    ((area / FLAME_AREA).ceil() as usize).clamp(1, MAX_FLAMES)
}

/// A number in [0, 1) from `seed` and `k`, the same on every machine.
pub fn unit(seed: u32, k: u32) -> f32 {
    (position_hash(Vec3::new(seed as f32, k as f32, 7.5)) >> 8) as f32 / (1u32 << 24) as f32
}

/// A flame card at `now`: how tall it is (before the pose's scale), and which of the two fire sprites it shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlamePose {
    pub height: f32,
    pub sprite: usize,
}

/// Flame `seed`'s height and sprite at `now`: 1.2 to 1.8 m, pulsing 0.85 to 1.15 of that at 3 to 5 Hz (its phase and rate from its seed),
/// alternating sprites every 0.12 s.
pub fn flame_pose(seed: u32, now: f32) -> FlamePose {
    let base = FLAME_HEIGHT.0 + (FLAME_HEIGHT.1 - FLAME_HEIGHT.0) * unit(seed, 1);
    let hz = 3.0 + 2.0 * unit(seed, 2);
    let pulse = 1.0 + 0.15 * (now * hz * TAU + unit(seed, 3) * TAU).sin();
    let step = (now / FLAME_SWAP_SECS).floor() as i64 + i64::from(seed);
    FlamePose { height: base * pulse, sprite: step.rem_euclid(2) as usize }
}

/// A Blaze light's flicker, 0.8 to 1.0 of its strength at 8 Hz.
pub fn flicker(seed: u32, now: f32) -> f32 {
    let wave = 0.5 + 0.5 * (now * 8.0 * TAU + unit(seed, 4) * TAU).sin();
    let wobble = 0.5 + 0.5 * (now * 13.0 * TAU + unit(seed, 5) * TAU).sin();
    0.8 + 0.2 * (0.7 * wave + 0.3 * wobble)
}

// ---- Where the parts go ----

/// The pipe from the line's first Valve to its last, a little above the floor: its two ends (the centre line at the pipe's height).
pub fn pipe_run(line: &FuelLine) -> Option<(Vec3, Vec3)> {
    let (first, last) = (line.valves.first()?, line.valves.last()?);
    if line.valves.len() < 2 {
        return None;
    }
    Some((first.1 + Vec3::Y * PIPE_RADIUS, last.1 + Vec3::Y * PIPE_RADIUS))
}

/// The part of the run from `a` to `b` inside `blaze`'s rectangle, as fractions of the run: `(from, to)`.
fn clip(min: Vec2, max: Vec2, a: Vec3, b: Vec3) -> Option<(f32, f32)> {
    let (mut from, mut to) = (0.0f32, 1.0f32);
    for (a, b, lo, hi) in [(a.x, b.x, min.x, max.x), (a.z, b.z, min.y, max.y)] {
        let d = b - a;
        if d.abs() < 1e-6 {
            if a < lo || a > hi {
                return None;
            }
            continue;
        }
        let (t0, t1) = ((lo - a) / d, (hi - a) / d);
        let (near, far) = if t0 < t1 { (t0, t1) } else { (t1, t0) };
        from = from.max(near);
        to = to.min(far);
    }
    (from < to).then_some((from, to))
}

/// The burner nozzles along a Blaze: one every `NOZZLE_GAP` along the part of the pipe that runs through it, centred in it, on the
/// pipe's top. A Blaze its line's pipe misses has them along its long middle.
pub fn nozzles(blaze: &super::fire::Blaze, run: Option<(Vec3, Vec3)>) -> Vec<Vec3> {
    let centre = blaze.centre();
    let size = blaze.max - blaze.min;
    let top = blaze.floor + 2.0 * PIPE_RADIUS;
    let (a, b) = run.filter(|&(a, b)| clip(blaze.min, blaze.max, a, b).is_some()).unwrap_or_else(|| {
        let along = if size.x >= size.y { Vec3::X } else { Vec3::Z };
        let half = along * size.x.max(size.y) / 2.0;
        (centre - half, centre + half)
    });
    let (from, to) = clip(blaze.min, blaze.max, a, b).unwrap_or((0.0, 1.0));
    let (start, end) = (a.lerp(b, from), a.lerp(b, to));
    let length = start.distance(end);
    let count = ((length / NOZZLE_GAP).floor() as usize).max(1);
    (0..count)
        .map(|i| {
            let along = (i as f32 + 0.5) / count as f32;
            let at = start.lerp(end, along);
            Vec3::new(at.x, top, at.z)
        })
        .collect()
}

/// A tile of a Blaze's kerb of hazard stripe: its middle on the floor, and its size (x, z).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KerbTile {
    pub at: Vec2,
    pub size: Vec2,
}

/// The kerb of `Decal::HazardStripe` that goes round a Blaze: a ring `KERB_WIDTH` wide on the outer edge of its bed, `BED_MARGIN` out from
/// the rectangle, in tiles about `KERB_TILE` long. The north and south runs take the corners.
pub fn kerb(min: Vec2, max: Vec2) -> Vec<KerbTile> {
    let (outer_min, outer_max) = (min - Vec2::splat(BED_MARGIN), max + Vec2::splat(BED_MARGIN));
    let (inner_min, inner_max) = (outer_min + Vec2::splat(KERB_WIDTH), outer_max - Vec2::splat(KERB_WIDTH));
    let mut tiles = Vec::new();
    let mut run = |from: f32, to: f32, across_at: f32, along_x: bool| {
        let n = (((to - from) / KERB_TILE).round() as usize).max(1);
        let length = (to - from) / n as f32;
        for i in 0..n {
            let along = from + length * (i as f32 + 0.5);
            tiles.push(if along_x {
                KerbTile { at: Vec2::new(along, across_at), size: Vec2::new(length, KERB_WIDTH) }
            } else {
                KerbTile { at: Vec2::new(across_at, along), size: Vec2::new(KERB_WIDTH, length) }
            });
        }
    };
    run(outer_min.x, outer_max.x, outer_min.y + KERB_WIDTH / 2.0, true);
    run(outer_min.x, outer_max.x, outer_max.y - KERB_WIDTH / 2.0, true);
    run(inner_min.y, inner_max.y, outer_min.x + KERB_WIDTH / 2.0, false);
    run(inner_min.y, inner_max.y, outer_max.x - KERB_WIDTH / 2.0, false);
    tiles
}

/// The nearest `cap` of `candidates` (each with its squared distance to the eye), nearest first.
pub fn nearest<T: Copy>(candidates: &[(T, f32)], cap: usize) -> Vec<T> {
    let mut sorted = candidates.to_vec();
    sorted.sort_by(|a, b| a.1.total_cmp(&b.1));
    sorted.into_iter().take(cap).map(|(item, _)| item).collect()
}

/// Which the Valve's lamp material is, from its pose.
fn lamp_index(lamp: Lamp) -> usize {
    Lamp::ALL.iter().position(|&l| l == lamp).expect("every lamp is in ALL")
}

/// The needle's turn about the gauge's face for `fraction` of its sweep: 135 degrees to the left of straight up at 0, 135 to the right at 1.
pub fn needle_angle(fraction: f32) -> f32 {
    (135.0 - 270.0 * fraction.clamp(0.0, 1.0)).to_radians()
}

/// The wheel's turn about its axle after `revs` revolutions (clockwise from the approach).
pub fn wheel_angle(revs: f32) -> f32 {
    -revs * TAU
}

// ---- What is in the world ----

/// A Blaze, and the point light over it.
#[derive(Component)]
pub struct BlazeSite {
    pub line: FuelLineId,
    pub centre: Vec3,
    pub min: Vec2,
    pub max: Vec2,
    pub floor: f32,
    pub nozzles: Vec<Vec3>,
    /// Which way the pipe runs through it (x, z), for the shimmer and the flames' spread.
    pub along: Vec2,
    /// Whether its cards have been spawned (they wait for the sprites).
    cards: bool,
    /// The phase's kind last frame, and the seconds since it lit, so a regrowth and an ignition can be timed here.
    last: Option<u8>,
    since_lit: f32,
    seed: u32,
}

/// What a Blaze shows now, written by `pose_blazes` for its cards to follow.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct BlazeShown {
    pub pose: BlazePose,
    /// `pose.flames` with the regrowth after lighting.
    pub flames: f32,
}

impl Default for BlazeShown {
    fn default() -> Self {
        Self { pose: blaze_pose(LinePhase::Lit), flames: 1.0 }
    }
}

/// A flame card standing on a nozzle of a Blaze (`site`), `index` among them.
#[derive(Component)]
pub struct FlameCard {
    pub site: Entity,
    seed: u32,
    base: Vec3,
}

/// A rising smoke card of a Blaze.
#[derive(Component)]
pub struct SmokeCard {
    pub site: Entity,
    pub index: usize,
    material: Handle<StandardMaterial>,
}

/// A blue pilot flame on a nozzle.
#[derive(Component)]
pub struct PilotCard {
    pub site: Entity,
    base: Vec3,
    seed: u32,
}

/// A card of heat shimmer along the pipe.
#[derive(Component)]
pub struct ShimmerCard {
    pub site: Entity,
    pub index: usize,
}

/// A Blaze's bed glow quad, which swaps between the two shared materials.
#[derive(Component)]
pub struct BedGlow {
    pub line: FuelLineId,
}

/// The glow strip under the grating over the Furnace Blaze, lit while the line is.
#[derive(Component)]
pub struct GlowStrip {
    pub line: FuelLineId,
}

/// A Valve's lamp, and the wheel and needle that turn.
#[derive(Component)]
pub struct ValveLamp {
    pub line: FuelLineId,
}

#[derive(Component)]
pub struct ValveWheel {
    pub line: FuelLineId,
}

#[derive(Component)]
pub struct ValveNeedle {
    pub line: FuelLineId,
}

/// A Valve part drawn from code until its kitbash glTF has loaded.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum StandIn {
    Stand,
    Wheel,
    Needle,
}

/// Two flame cards a burning Zombie wears: `part` 0 the chest, 1 the legs.
#[derive(Component)]
pub struct ZombieFlame {
    pub zombie: Entity,
    pub part: usize,
}

/// What the Fuel Lines' look is made of, shared by every Blaze and Valve: made when the Map is built.
#[derive(Resource, Clone)]
pub struct FireKit {
    quad: Handle<Mesh>,
    pub glow: Handle<StandardMaterial>,
    pub scorch: Handle<StandardMaterial>,
    pub lamps: [Handle<StandardMaterial>; 5],
}

/// The Kenney sprites the cards are drawn from, and whether they have all loaded. `stand_in` (no images, ready) is for headless tests.
#[derive(Resource, Clone, Default)]
pub struct FireSprites {
    fire: Vec<Handle<Image>>,
    smoke: Vec<Handle<Image>>,
    scorch: Handle<Image>,
    pub ready: bool,
    textured: bool,
}

impl FireSprites {
    /// Sprites that are ready at once with no image behind them: the cards are drawn untextured. For tests.
    #[cfg(test)]
    pub fn stand_in() -> Self {
        Self { fire: vec![Handle::default(); 2], smoke: vec![Handle::default(); 3], scorch: Handle::default(), ready: true, textured: false }
    }
}

/// The materials of the cards, made once the sprites are ready.
#[derive(Resource, Clone)]
pub struct CardKit {
    flames: [Handle<StandardMaterial>; 2],
    pilot: Handle<StandardMaterial>,
    shimmer: Handle<StandardMaterial>,
    smoke: Vec<Handle<StandardMaterial>>,
    quad: Handle<Mesh>,
}

/// How long each burning Zombie has left to burn, and the cards on the ones drawn.
#[derive(Resource, Default)]
pub struct Aflame {
    until: HashMap<Entity, f32>,
    cards: HashMap<Entity, [Entity; 2]>,
}

impl Aflame {
    /// How many Zombies are drawn burning.
    #[cfg(test)]
    pub fn drawn(&self) -> usize {
        self.cards.len()
    }
}

impl FireKit {
    pub fn new(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Self {
        let unlit = |colour: Color, alpha_mode: AlphaMode| StandardMaterial { base_color: colour, unlit: true, alpha_mode, double_sided: true, cull_mode: None, ..default() };
        Self {
            quad: meshes.add(Rectangle::new(1.0, 1.0)),
            // The bed glow: additive (2.4, 0.9, 0.25) x 1.5. Until its sprite loads it is the whole rectangle, unlit.
            glow: materials.add(unlit(Color::linear_rgb(3.6, 1.35, 0.375), AlphaMode::Add)),
            // And the scorch the blast's plain one is.
            scorch: materials.add(unlit(crate::weapons::blast::tint(BlastKind::Plain).scorch, AlphaMode::Blend)),
            lamps: Lamp::ALL.map(|lamp| {
                let (base, emissive) = lamp.colours();
                materials.add(StandardMaterial { base_color: base, emissive, perceptual_roughness: 0.3, ..default() })
            }),
        }
    }
}

// ---- Building ----

/// A mesh for a cylinder of `radius` and `length` along its y, whose UVs tile a material of `tile` metres round it and along it.
fn pipe_mesh(radius: f32, length: f32, tile: f32) -> Mesh {
    use bevy::mesh::VertexAttributeValues;
    let mut mesh = Cylinder::new(radius, length).mesh().resolution(16).build();
    let circumference = TAU * radius;
    if let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0) {
        for uv in uvs.iter_mut() {
            *uv = [uv[0] * circumference / tile, uv[1] * length / tile];
        }
    }
    mesh.with_generated_tangents().expect("a cylinder has everything tangents need")
}

/// A line's Blazes (each with its bed, kerb, glow quad, nozzles and light) and its feed pipe.
pub fn spawn_line(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, kit: &FireKit, id: FuelLineId, line: &FuelLine) {
    let rust = materials.add(StandardMaterial { base_color: Color::srgb(0.2, 0.17, 0.15), perceptual_roughness: 0.7, metallic: 0.5, ..default() });
    let soot = materials.add(StandardMaterial { base_color: Color::srgb(0.05, 0.045, 0.04), perceptual_roughness: 1.0, ..default() });
    let nozzle_mesh = meshes.add(Cylinder::new(0.05, NOZZLE_HEIGHT));
    let nozzle_material = materials.add(StandardMaterial { base_color: Color::srgb(0.03, 0.03, 0.03), perceptual_roughness: 0.6, metallic: 0.6, ..default() });
    let run = pipe_run(line);
    let along = run.map_or(Vec2::X, |(a, b)| Vec2::new(b.x - a.x, b.z - a.z).normalize_or(Vec2::X));

    // The feed pipe, from Valve to Valve: a code cylinder in `RustyMetal02`.
    if let Some((a, b)) = run {
        let length = a.distance(b);
        let middle = (a + b) / 2.0;
        let rotation = Quat::from_rotation_arc(Vec3::Y, (b - a) / length);
        commands.spawn((
            InRun,
            Name::new(format!("Feed pipe of the {}", line.name)),
            Mesh3d(meshes.add(pipe_mesh(PIPE_RADIUS, length, Material::RustyMetal02.tile()))),
            MeshMaterial3d(rust),
            Textured { material: Material::RustyMetal02, shape: Shape::Baked },
            Transform::from_translation(middle).with_rotation(rotation),
        ));
    }

    for (index, blaze) in line.blazes.iter().enumerate() {
        let centre = blaze.centre();
        let size = blaze.max - blaze.min;
        let bed = size + Vec2::splat(2.0 * BED_MARGIN);
        let seed = u32::from(id.0) * 16 + index as u32;
        let nozzle_spots = nozzles(blaze, run);

        // The bed: a thin `burned_ground_01` slab, its edge ringed with hazard stripe, and the glow quad over it.
        let slab_size = Vec3::new(bed.x, BED_THICKNESS, bed.y);
        let slab_at = centre + Vec3::Y * (BED_THICKNESS / 2.0);
        commands.spawn((
            InRun,
            Name::new(format!("Blaze bed of the {}", line.name)),
            Mesh3d(meshes.add(box_uv_mesh(slab_size, slab_at, Quat::IDENTITY, Material::BurnedGround01.tile()))),
            MeshMaterial3d(soot.clone()),
            Textured { material: Material::BurnedGround01, shape: Shape::Baked },
            Transform::from_translation(slab_at),
        ));
        let flat = Quat::from_rotation_x(-FRAC_PI_2);
        for tile in kerb(blaze.min, blaze.max) {
            commands.spawn((
                InRun,
                Name::new("Blaze kerb"),
                DecalPiece(Decal::HazardStripe),
                Transform::from_xyz(tile.at.x, blaze.floor + BED_THICKNESS + 0.006, tile.at.y).with_rotation(flat).with_scale(Vec3::new(tile.size.x, tile.size.y, 1.0)),
            ));
        }
        commands.spawn((
            InRun,
            Name::new(format!("Blaze glow of the {}", line.name)),
            BedGlow { line: id },
            Mesh3d(kit.quad.clone()),
            MeshMaterial3d(kit.glow.clone()),
            Transform::from_translation(centre + Vec3::Y * (BED_THICKNESS + 0.002)).with_rotation(flat).with_scale(Vec3::new(size.x + 0.4, size.y + 0.4, 1.0)),
            NotShadowCaster,
        ));
        for &spot in &nozzle_spots {
            commands.spawn((
                InRun,
                Name::new("Burner nozzle"),
                Mesh3d(nozzle_mesh.clone()),
                MeshMaterial3d(nozzle_material.clone()),
                Transform::from_translation(spot + Vec3::Y * (NOZZLE_HEIGHT / 2.0 - 0.02)),
            ));
        }
        // The Furnace Blaze burns under the Column Platform's grating: its glow strip is on the grating's underside.
        if blaze.zone == ZoneId::ColumnYard {
            commands.spawn((
                InRun,
                Name::new("Grating glow"),
                GlowStrip { line: id },
                Mesh3d(kit.quad.clone()),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(1.0, 0.45, 0.12),
                    emissive: LinearRgba::rgb(2.0, 0.9, 0.24),
                    unlit: true,
                    cull_mode: None,
                    double_sided: true,
                    ..default()
                })),
                Transform::from_xyz(centre.x, blaze.floor + super::map::STOREY - GRATING_UNDERSIDE, centre.z).with_rotation(Quat::from_rotation_x(FRAC_PI_2)).with_scale(Vec3::new(size.x, size.y, 1.0)),
                Visibility::default(),
                NotShadowCaster,
            ));
        }
        // The light: the entity is kept, and its intensity is set.
        let light_height = if blaze.zone == ZoneId::ColumnYard { FURNACE_LIGHT_HEIGHT } else { LIGHT_HEIGHT };
        commands.spawn((
            InRun,
            Name::new(format!("Blaze light of the {}", line.name)),
            BlazeSite {
                line: id,
                centre,
                min: blaze.min,
                max: blaze.max,
                floor: blaze.floor,
                nozzles: nozzle_spots,
                along,
                cards: false,
                last: None,
                since_lit: IGNITE_SECS,
                seed,
            },
            BlazeShown::default(),
            PointLight { color: BLAZE_COLOUR, intensity: BLAZE_LUMENS, range: BLAZE_RANGE, shadow_maps_enabled: false, ..default() },
            Transform::from_translation(centre + Vec3::Y * light_height),
            Visibility::default(),
        ));
    }
}

/// A Valve's stand, wheel, needle and lamp, as children of `valve` (which stands on the floor, its +z the way it faces).
pub fn dress_valve(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, kit: &FireKit, valve: Entity, id: FuelLineId) {
    let iron = materials.add(StandardMaterial { base_color: Color::srgb(0.12, 0.12, 0.13), metallic: 0.7, perceptual_roughness: 0.55, ..default() });
    let yellow = materials.add(StandardMaterial { base_color: Color::srgb(0.9, 0.7, 0.05), perceptual_roughness: 0.6, ..default() });
    let black = materials.add(StandardMaterial { base_color: Color::srgb(0.03, 0.03, 0.03), perceptual_roughness: 0.7, ..default() });
    let white = materials.add(StandardMaterial { base_color: Color::srgb(0.92, 0.92, 0.88), perceptual_roughness: 0.4, ..default() });
    let red = materials.add(StandardMaterial { base_color: Color::srgb(0.7, 0.05, 0.04), metallic: 0.3, perceptual_roughness: 0.5, ..default() });

    let part = |commands: &mut Commands, parent: Entity, mesh: Handle<Mesh>, material: &Handle<StandardMaterial>, at: Vec3, rotation: Quat| {
        commands.spawn((Mesh3d(mesh), MeshMaterial3d(material.clone()), Transform::from_translation(at).with_rotation(rotation), ChildOf(parent)));
    };
    let group = |commands: &mut Commands, parent: Entity, name: &'static str, stand_in: StandIn, at: Vec3| {
        commands.spawn((Name::new(name), stand_in, Transform::from_translation(at), Visibility::default(), ChildOf(parent))).id()
    };
    let turn_x = Quat::from_rotation_x(FRAC_PI_2);

    // The stand: foot, riser, gauge, hazard-striped mast.
    let stand = group(commands, valve, "Valve stand", StandIn::Stand, Vec3::ZERO);
    part(commands, stand, meshes.add(Cylinder::new(0.2, 0.06)), &iron, Vec3::new(0.0, 0.03, 0.0), Quat::IDENTITY);
    part(commands, stand, meshes.add(Cylinder::new(0.075, RISER)), &iron, Vec3::new(0.0, RISER / 2.0, 0.0), Quat::IDENTITY);
    part(commands, stand, meshes.add(Cuboid::new(0.2, 0.14, 0.2)), &iron, Vec3::new(0.0, RISER - 0.05, 0.0), Quat::IDENTITY);
    part(commands, stand, meshes.add(Cylinder::new(0.09, 0.05)), &iron, GAUGE_AT, turn_x);
    part(commands, stand, meshes.add(Cylinder::new(0.082, 0.006)), &white, GAUGE_AT + Vec3::Z * 0.027, turn_x);
    part(commands, stand, meshes.add(Cylinder::new(0.05, MAST_HEIGHT)), &yellow, Vec3::new(MAST_AT.x, MAST_HEIGHT / 2.0, MAST_AT.y), Quat::IDENTITY);
    let band = meshes.add(Cylinder::new(0.054, 0.2));
    for i in (1..12).step_by(2) {
        part(commands, stand, band.clone(), &black, Vec3::new(MAST_AT.x, 0.2 * (i as f32 + 0.5), MAST_AT.y), Quat::IDENTITY);
    }
    part(commands, stand, meshes.add(Cylinder::new(0.12, 0.02)), &iron, Vec3::new(MAST_AT.x, MAST_HEIGHT + 0.27, MAST_AT.y), Quat::IDENTITY);

    // The wheel: a pivot at the hub, turned about the axle.
    let wheel = commands.spawn((Name::new("Valve wheel"), ValveWheel { line: id }, Transform::from_translation(HUB_AT), Visibility::default(), ChildOf(valve))).id();
    let wheel_group = group(commands, wheel, "Valve wheel stand-in", StandIn::Wheel, Vec3::ZERO);
    part(commands, wheel_group, meshes.add(Torus::new(WHEEL_RADIUS - 0.025, WHEEL_RADIUS + 0.025)), &red, Vec3::ZERO, turn_x);
    part(commands, wheel_group, meshes.add(Cylinder::new(0.05, 0.08)), &iron, Vec3::ZERO, turn_x);
    let spoke = meshes.add(Cuboid::new(0.035, WHEEL_RADIUS - 0.03, 0.04));
    for k in 0..4 {
        let a = k as f32 * TAU / 4.0;
        part(commands, wheel_group, spoke.clone(), &red, Quat::from_rotation_z(a) * Vec3::Y * ((WHEEL_RADIUS - 0.03) / 2.0), Quat::from_rotation_z(a));
    }

    // The needle: a pivot at the gauge's face, turned about its depth.
    let needle = commands
        .spawn((Name::new("Valve needle"), ValveNeedle { line: id }, Transform::from_translation(GAUGE_AT + Vec3::Z * NEEDLE_PROUD), Visibility::default(), ChildOf(valve)))
        .id();
    let needle_group = group(commands, needle, "Valve needle stand-in", StandIn::Needle, Vec3::ZERO);
    part(commands, needle_group, meshes.add(Cuboid::new(0.012, 0.075, 0.006)), &red, Vec3::Y * 0.0375, Quat::IDENTITY);

    // The lamp: emissive only, no light. Its material is the pose's.
    commands.spawn((
        Name::new("Valve lamp"),
        ValveLamp { line: id },
        Mesh3d(meshes.add(Sphere::new(0.1).mesh().ico(2).expect("a small sphere"))),
        MeshMaterial3d(kit.lamps[lamp_index(Lamp::RedOrange)].clone()),
        Transform::from_translation(LAMP_AT),
        NotShadowCaster,
        ChildOf(valve),
    ));
}

// ---- Systems ----

/// Loads the sprites once there is an asset server and every file is on disk.
fn load_sprites(mut commands: Commands, asset_server: Option<Res<AssetServer>>, images: Option<Res<Assets<Image>>>, mut tried: Local<bool>) {
    if *tried {
        return;
    }
    let Some(asset_server) = asset_server.filter(|_| images.is_some()) else { return };
    *tried = true;
    let names: Vec<&str> = FIRE_SPRITES.iter().chain(&SMOKE_SPRITES).chain(&[SCORCH_SPRITE]).copied().collect();
    if !names.iter().all(|name| on_disk(&sprite_file(name))) {
        debug!("fire sprites missing, the Blazes have no flame cards");
        return;
    }
    let load = |name: &str| -> Handle<Image> { asset_server.load(sprite_file(name)) };
    commands.insert_resource(FireSprites {
        fire: FIRE_SPRITES.iter().map(|name| load(name)).collect(),
        smoke: SMOKE_SPRITES.iter().map(|name| load(name)).collect(),
        scorch: load(SCORCH_SPRITE),
        ready: false,
        textured: true,
    });
}

/// Marks the sprites ready once every image has loaded.
fn settle_sprites(mut sprites: ResMut<FireSprites>, asset_server: Option<Res<AssetServer>>) {
    if sprites.ready {
        return;
    }
    let Some(asset_server) = asset_server else { return };
    let ready = sprites.fire.iter().chain(&sprites.smoke).chain(std::iter::once(&sprites.scorch)).all(|image| asset_server.is_loaded_with_dependencies(image));
    if ready {
        sprites.ready = true;
    }
}

/// Makes the cards' materials once the sprites are ready.
fn build_card_kit(mut commands: Commands, sprites: Res<FireSprites>, kit: Res<FireKit>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let card = |image: &Handle<Image>, colour: Color, alpha_mode: AlphaMode, textured: bool| StandardMaterial {
        base_color: colour,
        base_color_texture: textured.then(|| image.clone()),
        unlit: true,
        alpha_mode,
        double_sided: true,
        cull_mode: None,
        ..default()
    };
    let tint = crate::weapons::blast::tint(BlastKind::Plain);
    let textured = sprites.textured;
    commands.insert_resource(CardKit {
        flames: [0, 1].map(|i| materials.add(card(&sprites.fire[i], tint.fire, AlphaMode::Add, textured))),
        // The blue pilot: a small fire_02 tinted (0.3, 0.5, 1.0).
        pilot: materials.add(card(&sprites.fire[1], Color::linear_rgb(0.6, 1.0, 2.0), AlphaMode::Add, textured)),
        // The heat shimmer: faint, additive.
        shimmer: materials.add(card(&sprites.smoke[0], Color::linear_rgb(0.05, 0.05, 0.055), AlphaMode::Add, textured)),
        smoke: (0..SMOKE_SPRITES.len()).map(|i| materials.add(card(&sprites.smoke[i], Color::srgba(0.08, 0.07, 0.06, 0.6), AlphaMode::Blend, textured))).collect(),
        quad: kit.quad.clone(),
    });
}

/// Gives the two bed materials their `scorch_01` image once it has loaded (a fresh `FireKit` every Run needs it again).
fn texture_bed(kit: Res<FireKit>, sprites: Res<FireSprites>, mut materials: ResMut<Assets<StandardMaterial>>) {
    if !sprites.ready || !sprites.textured {
        return;
    }
    for handle in [&kit.glow, &kit.scorch] {
        if let Some(mut material) = materials.get_mut(handle)
            && material.base_color_texture.is_none()
        {
            material.base_color_texture = Some(sprites.scorch.clone());
        }
    }
}

/// Spawns each Blaze's cards once the card materials exist.
fn spawn_cards(mut commands: Commands, cards: Res<CardKit>, mut materials: ResMut<Assets<StandardMaterial>>, mut sites: Query<(Entity, &mut BlazeSite)>) {
    for (entity, mut site) in &mut sites {
        if site.cards {
            continue;
        }
        site.cards = true;
        let size = site.max - site.min;
        let side = Vec3::new(-site.along.y, 0.0, site.along.x);
        for index in 0..flame_count(size.x * size.y) {
            let seed = site.seed * 8 + index as u32;
            let nozzle = site.nozzles[index % site.nozzles.len()];
            let spread = (unit(seed, 6) - 0.5) * 1.8;
            let mut base = nozzle + side * spread;
            base.x = base.x.clamp(site.min.x, site.max.x);
            base.z = base.z.clamp(site.min.y, site.max.y);
            base.y = site.floor + 2.0 * PIPE_RADIUS;
            commands.spawn((
                InRun,
                Name::new("Flame card"),
                FlameCard { site: entity, seed, base },
                Mesh3d(cards.quad.clone()),
                MeshMaterial3d(cards.flames[0].clone()),
                Transform::from_translation(base).with_scale(Vec3::splat(0.001)),
                Visibility::default(),
                NotShadowCaster,
            ));
        }
        for index in 0..SMOKE_CARDS {
            let own = materials.get(&cards.smoke[index % cards.smoke.len()]).cloned().unwrap_or_default();
            let material = materials.add(own);
            commands.spawn((
                InRun,
                Name::new("Smoke card"),
                SmokeCard { site: entity, index, material: material.clone() },
                Mesh3d(cards.quad.clone()),
                MeshMaterial3d(material),
                Transform::from_translation(site.centre).with_scale(Vec3::splat(0.001)),
                Visibility::default(),
                NotShadowCaster,
            ));
        }
        for (k, &spot) in site.nozzles.iter().enumerate() {
            commands.spawn((
                InRun,
                Name::new("Pilot card"),
                PilotCard { site: entity, base: spot + Vec3::Y * 0.2, seed: site.seed * 16 + k as u32 },
                Mesh3d(cards.quad.clone()),
                MeshMaterial3d(cards.pilot.clone()),
                Transform::from_translation(spot).with_scale(Vec3::splat(0.001)),
                Visibility::default(),
                NotShadowCaster,
            ));
        }
        for index in 0..SHIMMER_CARDS {
            commands.spawn((
                InRun,
                Name::new("Shimmer card"),
                ShimmerCard { site: entity, index },
                Mesh3d(cards.quad.clone()),
                MeshMaterial3d(cards.shimmer.clone()),
                Transform::from_translation(site.centre).with_scale(Vec3::splat(0.001)),
                Visibility::default(),
                NotShadowCaster,
            ));
        }
    }
}

/// Where the eye is, if there is a camera.
fn eye(cameras: &Query<&GlobalTransform, With<Camera3d>>) -> Option<Vec3> {
    cameras.iter().next().map(GlobalTransform::translation)
}

/// Turns a card at `at` to face `eye`: upright (about y only) or fully.
fn face(at: Vec3, eye: Option<Vec3>, upright: bool) -> Option<Quat> {
    let mut toward = eye? - at;
    if upright {
        toward.y = 0.0;
    }
    (toward.length_squared() > 1e-6).then(|| Transform::IDENTITY.looking_to(-toward, Vec3::Y).rotation)
}

/// Poses every Blaze from its line's phase: its light, its bed, and what its cards are told to show. A line that lights while it was
/// Flaring up (seen here, so every machine sees it) regrows its flames over `IGNITE_SECS` and throws a cosmetic blast.
fn pose_blazes(
    mut commands: Commands,
    lines: Res<FuelLines>,
    time: Res<Time>,
    kit: Res<FireKit>,
    explosive: Option<Res<ExplosiveAssets>>,
    bits: Query<(), With<BlastBit>>,
    mut sites: Query<(&mut BlazeSite, &mut BlazeShown, &mut PointLight)>,
    mut beds: Query<(&BedGlow, &mut MeshMaterial3d<StandardMaterial>), Without<GlowStrip>>,
    mut strips: Query<(&GlowStrip, &mut Visibility)>,
) {
    let now = time.elapsed_secs();
    for (mut site, mut shown, mut light) in &mut sites {
        let phase = lines.phase(site.line);
        let kind = match phase {
            LinePhase::Lit => 0,
            LinePhase::Shut { .. } => 1,
            LinePhase::FlareUp { .. } => 2,
        };
        match (site.last, kind) {
            // Lit from a Flare-up: the flames regrow and a cosmetic blast of radius 1.5 goes off (its scorch skipped).
            (Some(2), 0) => {
                site.since_lit = 0.0;
                if let Some(look) = explosive.as_ref().and_then(|assets| assets.blast.as_ref()) {
                    spawn_blast(&mut commands, look, bits.iter().count(), &[], site.centre + Vec3::Y * 0.3, 1.5, None, BlastKind::Plain);
                }
            }
            (_, 0) => site.since_lit = (site.since_lit + time.delta_secs()).min(IGNITE_SECS),
            _ => site.since_lit = 0.0,
        }
        site.last = Some(kind);
        let pose = blaze_pose(phase);
        let ramp = if kind == 0 { ignite_ramp(site.since_lit) } else { 1.0 };
        let wanted = BlazeShown { pose, flames: pose.flames * ramp };
        if *shown != wanted {
            *shown = wanted;
        }
        let intensity = BLAZE_LUMENS * pose.light * flicker(site.seed, now);
        if (light.intensity - intensity).abs() > 1.0 {
            light.intensity = intensity;
        }
    }
    for (bed, mut handle) in &mut beds {
        let glowing = lines.phase(bed.line);
        let want = if blaze_pose(glowing).glow >= 0.5 { &kit.glow } else { &kit.scorch };
        if handle.0 != *want {
            handle.0 = want.clone();
        }
    }
    for (strip, mut visibility) in &mut strips {
        visibility.set_if_neq(if lines.phase(strip.line) == LinePhase::Lit { Visibility::Inherited } else { Visibility::Hidden });
    }
}

/// Moves the flame, smoke, pilot and shimmer cards by what their Blaze shows.
fn animate_cards(
    time: Res<Time>,
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    cards: Option<Res<CardKit>>,
    sites: Query<(&BlazeSite, &BlazeShown)>,
    mut flames: Query<(&FlameCard, &mut Transform, &mut Visibility, &mut MeshMaterial3d<StandardMaterial>), (Without<SmokeCard>, Without<PilotCard>, Without<ShimmerCard>)>,
    mut smoke: Query<(&SmokeCard, &mut Transform, &mut Visibility), (Without<FlameCard>, Without<PilotCard>, Without<ShimmerCard>)>,
    mut pilots: Query<(&PilotCard, &mut Transform, &mut Visibility), (Without<FlameCard>, Without<SmokeCard>, Without<ShimmerCard>)>,
    mut shimmers: Query<(&ShimmerCard, &mut Transform, &mut Visibility), (Without<FlameCard>, Without<SmokeCard>, Without<PilotCard>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(cards) = cards else { return };
    let now = time.elapsed_secs();
    let eye = eye(&cameras);
    for (card, mut transform, mut visibility, mut material) in &mut flames {
        let Ok((_, shown)) = sites.get(card.site) else { continue };
        let pose = flame_pose(card.seed, now);
        let scale = shown.flames;
        let height = pose.height * scale;
        visibility.set_if_neq(if scale > 0.01 { Visibility::Inherited } else { Visibility::Hidden });
        if scale <= 0.01 {
            continue;
        }
        transform.translation = card.base + Vec3::Y * (height / 2.0 - 0.1 * scale);
        transform.scale = Vec3::new(height * 0.6, height, 1.0);
        if let Some(rotation) = face(transform.translation, eye, true) {
            transform.rotation = rotation;
        }
        let want = &cards.flames[pose.sprite];
        if material.0 != *want {
            material.0 = want.clone();
        }
    }
    for (card, mut transform, mut visibility) in &mut smoke {
        let Ok((site, shown)) = sites.get(card.site) else { continue };
        let show = card.index < shown.pose.smoke.cards && shown.pose.smoke.alpha > 0.01;
        visibility.set_if_neq(if show { Visibility::Inherited } else { Visibility::Hidden });
        if !show {
            continue;
        }
        // Each card rises through a 4 s life, a third of a life behind the one before, and billows as it goes.
        let t = (now / SMOKE_SECS + card.index as f32 / SMOKE_CARDS as f32 + unit(site.seed, 7)).rem_euclid(1.0);
        let drift = Vec3::new(unit(site.seed, 8 + card.index as u32) - 0.5, 0.0, unit(site.seed, 12 + card.index as u32) - 0.5) * 2.0;
        transform.translation = site.centre + drift + Vec3::Y * (1.0 + t * 2.6);
        transform.scale = Vec3::splat(1.5 * envelope(BlastShape::Billow, t));
        if let Some(rotation) = face(transform.translation, eye, false) {
            transform.rotation = rotation;
        }
    }
    for (card, mut transform, mut visibility) in &mut pilots {
        let Ok((_, shown)) = sites.get(card.site) else { continue };
        visibility.set_if_neq(if shown.pose.pilot > 0.0 { Visibility::Inherited } else { Visibility::Hidden });
        if shown.pose.pilot <= 0.0 {
            continue;
        }
        let flutter = 1.0 + 0.2 * (now * 30.0 + unit(card.seed, 9) * TAU).sin();
        transform.translation = card.base;
        transform.scale = Vec3::new(0.25, 0.4, 1.0) * flutter;
        if let Some(rotation) = face(transform.translation, eye, true) {
            transform.rotation = rotation;
        }
    }
    for (card, mut transform, mut visibility) in &mut shimmers {
        let Ok((site, shown)) = sites.get(card.site) else { continue };
        visibility.set_if_neq(if shown.pose.shimmer > 0.0 { Visibility::Inherited } else { Visibility::Hidden });
        if shown.pose.shimmer <= 0.0 {
            continue;
        }
        // Three faint cards along the pipe, wobbling in size and sideways on the CPU.
        let along = Vec3::new(site.along.x, 0.0, site.along.y);
        let spot = (card.index as f32 - (SHIMMER_CARDS as f32 - 1.0) / 2.0) * 1.4;
        let wobble = (now * 5.0 + card.index as f32 * 2.1).sin();
        transform.translation = site.centre + along * spot + Vec3::new(0.15 * wobble, 0.9 + 0.1 * wobble, 0.15 * (now * 3.7 + card.index as f32).cos());
        transform.scale = Vec3::splat(1.8 * (1.0 + 0.12 * wobble));
        if let Some(rotation) = face(transform.translation, eye, false) {
            transform.rotation = rotation;
        }
    }
    // The smoke's colour follows its Blaze's alpha.
    for (card, ..) in &smoke {
        let Ok((_, shown)) = sites.get(card.site) else { continue };
        if let Some(mut paint) = materials.get_mut(&card.material)
            && (paint.base_color.alpha() - shown.pose.smoke.alpha).abs() > 1e-3
        {
            paint.base_color = paint.base_color.with_alpha(shown.pose.smoke.alpha);
        }
    }
}

/// Poses every Valve from its line: the lamp's colour, the needle and the wheel. A Sabotaged line is red and fast.
fn pose_valves(
    lines: Res<FuelLines>,
    time: Res<Time>,
    kit: Res<FireKit>,
    mut lamps: Query<(&ValveLamp, &mut MeshMaterial3d<StandardMaterial>)>,
    mut wheels: Query<(&ValveWheel, &mut Transform), Without<ValveNeedle>>,
    mut needles: Query<(&ValveNeedle, &mut Transform), Without<ValveWheel>>,
) {
    let now = time.elapsed_secs();
    for (lamp, mut handle) in &mut lamps {
        let pose = valve_pose(lines.phase(lamp.line), lines.sabotaged().contains(lamp.line), now);
        let want = &kit.lamps[lamp_index(pose.lamp)];
        if handle.0 != *want {
            handle.0 = want.clone();
        }
    }
    for (wheel, mut transform) in &mut wheels {
        let pose = valve_pose(lines.phase(wheel.line), lines.sabotaged().contains(wheel.line), now);
        transform.rotation = Quat::from_rotation_z(wheel_angle(pose.wheel));
    }
    for (needle, mut transform) in &mut needles {
        let pose = valve_pose(lines.phase(needle.line), lines.sabotaged().contains(needle.line), now);
        transform.rotation = Quat::from_rotation_z(needle_angle(pose.needle));
    }
}

/// Swaps a Valve part's code-built stand-in for its kitbash glTF once the file is on disk (and there is a world to load it).
fn dress_models(
    mut commands: Commands,
    asset_server: Option<Res<AssetServer>>,
    scenes: Option<Res<Assets<WorldAsset>>>,
    stand_ins: Query<(Entity, &StandIn, &ChildOf), Added<StandIn>>,
) {
    let Some(server) = asset_server.filter(|_| scenes.is_some()) else { return };
    for (entity, stand_in, parent) in &stand_ins {
        let path = match stand_in {
            StandIn::Stand => STAND_MODEL,
            StandIn::Wheel => WHEEL_MODEL,
            StandIn::Needle => NEEDLE_MODEL,
        };
        if !on_disk(path) {
            continue;
        }
        commands.entity(entity).despawn();
        commands.spawn((Name::new("Model"), WorldAssetRoot(server.load(GltfAssetLabel::Scene(0).from_asset(path.to_string()))), Transform::default(), Visibility::default(), ChildOf(parent.parent())));
    }
}

/// The Zombies whose feet are in a Lit Blaze burn for `BURN_AFTER_SECS` after it, by the one feet rule on every machine; the nearest
/// `MAX_BURNING_ZOMBIES` of them wear two flame cards (chest and legs).
fn burning_zombies(
    mut commands: Commands,
    lines: Res<FuelLines>,
    map: Option<Res<MapId>>,
    time: Res<Time>,
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    cards: Option<Res<CardKit>>,
    mut burning: ResMut<Aflame>,
    zombies: Query<(Entity, &Zombie, &Transform), Without<Dying>>,
    mut flames: Query<(&ZombieFlame, &mut Transform, &mut MeshMaterial3d<StandardMaterial>), Without<Zombie>>,
) {
    let Some(map) = map else { return };
    let now = time.elapsed_secs();
    let lit = lines.burning();
    let layout = map.layout();
    let eye = eye(&cameras);
    // Who is burning: in a Lit Blaze now, or was within the last `BURN_AFTER_SECS`.
    for (entity, zombie, transform) in &zombies {
        let feet = transform.translation - Vec3::Y * zombie.half_height();
        if burns_in_a_blaze(zombie, false) && blaze_at(layout, lit, feet).is_some() {
            burning.until.insert(entity, now + BURN_AFTER_SECS);
        }
    }
    burning.until.retain(|&entity, &mut until| until > now && zombies.contains(entity));
    let candidates: Vec<(Entity, f32)> = burning
        .until
        .keys()
        .filter_map(|&entity| zombies.get(entity).ok())
        .map(|(entity, _, transform)| (entity, eye.map_or(0.0, |eye| eye.distance_squared(transform.translation))))
        .collect();
    let drawn = nearest(&candidates, MAX_BURNING_ZOMBIES);
    // Those no longer drawn lose their cards; those newly drawn get them.
    let gone: Vec<Entity> = burning.cards.keys().copied().filter(|entity| !drawn.contains(entity)).collect();
    for entity in gone {
        if let Some(pair) = burning.cards.remove(&entity) {
            for card in pair {
                commands.entity(card).try_despawn();
            }
        }
    }
    let Some(cards) = cards else { return };
    for &entity in &drawn {
        if burning.cards.contains_key(&entity) {
            continue;
        }
        let at = zombies.get(entity).map_or(Vec3::ZERO, |(_, _, transform)| transform.translation);
        let pair = [0, 1].map(|part| {
            commands
                .spawn((InRun, Name::new("Zombie flame"), ZombieFlame { zombie: entity, part }, Mesh3d(cards.quad.clone()), MeshMaterial3d(cards.flames[part].clone()), Transform::from_translation(at).with_scale(Vec3::splat(0.001)), NotShadowCaster))
                .id()
        });
        burning.cards.insert(entity, pair);
    }
    for (flame, mut transform, mut material) in &mut flames {
        let Ok((_, zombie, body)) = zombies.get(flame.zombie) else { continue };
        let half = zombie.half_height();
        let pose = flame_pose((flame.zombie.to_bits() as u32).wrapping_mul(2).wrapping_add(flame.part as u32), now);
        // The chest is high, the legs low: the Zombie's own height, so a Crawler's flames lie low.
        let feet = body.translation - Vec3::Y * half;
        let at = feet + Vec3::Y * (2.0 * half * if flame.part == 0 { 0.7 } else { 0.25 });
        let size = (2.0 * half / 1.8).max(0.4) * (pose.height / 1.5);
        transform.translation = at;
        transform.scale = Vec3::new(size * 0.6, size, 1.0);
        if let Some(rotation) = face(at, eye, true) {
            transform.rotation = rotation;
        }
        let want = &cards.flames[pose.sprite];
        if material.0 != *want {
            material.0 = want.clone();
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;

pub struct FireLookPlugin;

impl Plugin for FireLookPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Aflame>()
            .add_systems(
                Update,
                (
                    load_sprites.run_if(not(resource_exists::<FireSprites>)),
                    (settle_sprites, build_card_kit.run_if(not(resource_exists::<CardKit>))).chain().run_if(resource_exists::<FireSprites>),
                    texture_bed.run_if(resource_exists::<FireSprites>),
                    dress_models,
                )
                    .run_if(world_runs)
                    .run_if(resource_exists::<FireKit>),
            )
            .add_systems(
                Update,
                (
                    pose_blazes,
                    spawn_cards.run_if(resource_exists::<CardKit>),
                    animate_cards,
                    pose_valves,
                    burning_zombies,
                )
                    .chain()
                    .after(FireClock)
                    .after(FireShown)
                    .run_if(world_runs)
                    .run_if(resource_exists::<FireKit>),
            );
    }
}
