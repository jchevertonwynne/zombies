//! Shifting: Pieces of the world that turn about vertical hinges, or slide along a straight floor
//! rail, between fixed Rests, and the Levers, the Tide, the Quarantine or the Wheels that drive them.
//! The Polder's four Swing Bridges are turned by Levers; its two Lock Gates by the Tide, which gives
//! each the Rest it stands at in every phase and swings it along with the water; the Sanatorium's six
//! Quarantine Doors by the Quarantine, which shuts a Wing's pair of them and opens them again; the
//! Archive's five Ranges (the Moving Stacks) by Wheels, each of which rolls its Range to open its own
//! Aisle.
//!
//! A Piece is one or more Leaves, each a solid box (a deck with rails, a gate, a Bookcase) that turns
//! about its own vertical hinge or slides along its rail. The Host decides everything: a Lever pull starts a cycle of Warning, Waiting
//! (while any Player is on a Leaf or in its sweep), Swinging and Cooling. Every machine poses its
//! own Leaves from the phase it is told, and nothing is carried: a body on a deck while it turns
//! is pushed out by the move step.

use avian3d::prelude::*;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use bevy_transform_interpolation::TranslationEasingState;

use crate::audio::PlayCue;
use crate::audio::cues::Cue;
use crate::authority::HostSimulation;
use crate::economy::buy::{Buyable, Purchased, Refused};
use crate::enemies::sapper::Sabotaged;
use crate::enemies::target::{Players, half_height_above_feet};
use crate::enemies::zombie::{Attack, DamageZombie, Dying, HitZone, Rising, Zombie};
use crate::layers::GameLayer;
use crate::maps::door::Door;
use crate::enemies::nav::Route;
use crate::enemies::special::special_spawn_candidates;
use crate::maps::map::{Layout, MapId, SpawnKind, SpawnPoint};
use crate::maps::quarantine::{Quarantine, QuarantineClock, Stage, WingId};
use crate::maps::tide::{Tide, TidePhase, TideTurn, TurnState};
use crate::maps::zone::ActiveZones;
use crate::maps::zone::ZoneId;
use crate::players::player::{Locomotion, Player, Stance, Teleport};
use crate::state::{InRun, world_runs};
use crate::tuning::*;

/// A Piece's place in `Layout.shifting`, the same on every machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PieceId(pub u8);

/// How a Leaf moves between Rests.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Motion {
    /// It turns about a vertical hinge: a Rest gives its yaw.
    Hinge,
    /// It slides along a straight floor rail, this unit direction (x, z): a Rest gives the metres
    /// from the `hinge`.
    Rail(Vec2),
}

/// One solid box that turns about a vertical hinge or slides along a rail: a deck with its rails, a
/// gate leaf or a Bookcase.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Leaf {
    /// The hinge's spot in the world, or the rail's datum: where the Leaf is at pose 0. Its height
    /// is the Leaf's datum.
    pub hinge: Vec3,
    /// The box's full size.
    pub size: Vec3,
    /// The box's centre from the hinge, with the Leaf at pose 0 (yaw 0 for a Hinge Leaf).
    pub offset: Vec3,
    pub motion: Motion,
}

impl Leaf {
    /// Where the Leaf's frame (its hinge) is at `pose`: the hinge itself, or along the rail.
    pub fn origin(&self, pose: f32) -> Vec3 {
        match self.motion {
            Motion::Hinge => self.hinge,
            Motion::Rail(dir) => self.hinge + Vec3::new(dir.x, 0.0, dir.y) * pose,
        }
    }

    /// How the Leaf is turned at `pose`: by its yaw, or not at all on a rail.
    pub fn rotation(&self, pose: f32) -> Quat {
        match self.motion {
            Motion::Hinge => Quat::from_rotation_y(pose),
            Motion::Rail(_) => Quat::IDENTITY,
        }
    }

    /// Where the Leaf's centre is at `pose`.
    pub fn centre(&self, pose: f32) -> Vec3 {
        self.origin(pose) + self.rotation(pose) * self.offset
    }

    /// The height of the top of its deck.
    pub fn top(&self) -> f32 {
        self.hinge.y + self.offset.y + self.size.y / 2.0
    }

    /// Whether the point `at` (flat) is over the box at `pose`, `margin` out from its edge.
    pub fn over(&self, at: Vec3, pose: f32, margin: f32) -> bool {
        let local = self.rotation(pose).inverse() * (at - self.origin(pose)) - self.offset;
        local.x.abs() <= self.size.x / 2.0 + margin && local.z.abs() <= self.size.z / 2.0 + margin
    }

    /// Whether the Leaf is a door, standing from its hinge's floor up, not a deck to walk on.
    pub fn is_door(&self) -> bool {
        self.size.y > 1.0
    }

    /// Whether feet at `feet` are at the heights a body on the deck or in the water below it is.
    /// A door's reach is the floor it stands on (a body a storey up is not in its way).
    pub fn height_in_reach(&self, feet: Vec3) -> bool {
        if self.is_door() {
            let bottom = self.hinge.y + self.offset.y - self.size.y / 2.0;
            return (bottom - SWING_DOOR_REACH_BELOW..=self.top()).contains(&feet.y);
        }
        (self.top() - SWING_REACH_BELOW..=self.top() + SWING_REACH_ABOVE).contains(&feet.y)
    }
}

/// Where a Piece can stand still: a yaw or a rail position for each Leaf, and the Zone joins it
/// makes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rest {
    /// The yaw of a Hinge Leaf, or the metres along the rail from the `hinge` of a Rail Leaf.
    pub at: &'static [f32],
    pub joins: &'static [(ZoneId, ZoneId)],
}

/// What drives a Piece.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Driver {
    /// Levers: the Zone each stands in, where (on the floor) and which way it faces (x, z). A
    /// Lever steps to the next Rest, wrapping.
    Levers(&'static [(ZoneId, Vec3, Vec2)]),
    /// The Tide: the Rest it stands at in each phase (Low, Rising, High, Falling). It Swings only
    /// on a Turn whose new phase has a different Rest, over the Turn's own seconds, and has no
    /// Lever, no cooldown and nothing to Sabotage.
    Tide([u8; 4]),
    /// The Quarantine: it shuts when this Wing seals (Rest 1) and stands Open (Rest 0) otherwise.
    /// It Swings only as the Quarantine's stage says, over `SWING_SECS`, with no Lever, no wait and no
    /// cooldown, and nothing to Sabotage.
    Quarantine(WingId),
    /// Wheels: each opens its own Rest, free, for any Player who is up, once the Range is idle. A
    /// Range has no Levers.
    Wheels(&'static [Wheel]),
}

/// A hand Wheel at the mouth of an Aisle: where it stands, the way it faces (x, z) and the Rest of
/// the Range it rolls open.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wheel {
    /// The Zone it stands in.
    pub zone: ZoneId,
    /// Its spot on the floor.
    pub at: Vec3,
    pub facing: Vec2,
    /// The Rest it rolls the Range to: where its Aisle is open.
    pub rest: u8,
}

impl Wheel {
    /// The Rest a use of this Wheel rolls to, if it does anything now: only an idle Range whose
    /// Aisle at this Wheel is shut.
    pub fn opens(&self, phase: PiecePhase) -> Option<u8> {
        match phase {
            PiecePhase::Rest(at) if at != self.rest => Some(self.rest),
            _ => None,
        }
    }
}

/// A Piece of world that shifts between Rests.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Piece {
    /// The Zone it belongs to.
    pub zone: ZoneId,
    pub leaves: &'static [Leaf],
    pub rests: &'static [Rest],
    /// The Rest it stands at when the Run begins.
    pub start: u8,
    pub driver: Driver,
    /// The gate Door (named by the two Zones it guards, as `Door::gate` is) that must be open
    /// for a Rest's joins to count. It joins nothing itself.
    pub door: Option<(ZoneId, ZoneId)>,
}

/// The pose (a yaw, or metres along a rail) `elapsed` seconds into a Swing from `from` to `to`: a
/// smoothstep that lands exactly.
pub fn pose(from: f32, to: f32, elapsed: f32) -> f32 {
    let t = (elapsed / SWING_SECS).clamp(0.0, 1.0);
    if t >= 1.0 {
        to
    } else if t <= 0.0 {
        from
    } else {
        from + (to - from) * t * t * (3.0 - 2.0 * t)
    }
}

impl Piece {
    /// The Rest after `rest`, wrapping.
    pub fn next_rest(&self, rest: u8) -> u8 {
        (rest + 1) % self.rests.len() as u8
    }

    /// Every Leaf's pose `elapsed` seconds into a Swing from Rest `from` to Rest `to`.
    pub fn poses(&self, from: u8, to: u8, elapsed: f32) -> Vec<f32> {
        let (a, b) = (self.rests[usize::from(from)].at, self.rests[usize::from(to)].at);
        a.iter().zip(b).map(|(&a, &b)| pose(a, b, elapsed)).collect()
    }

    /// How long its Levers or Wheels are dead after a Swing.
    pub fn cooldown(&self) -> f32 {
        match self.driver {
            Driver::Wheels(_) => RANGE_COOLDOWN_SECS,
            _ => LEVER_COOLDOWN_SECS,
        }
    }

    /// Whether feet are on a Leaf or in its sweep (the ground it passes over between two Rests),
    /// at deck height or in the water below it.
    pub fn in_way(&self, feet: Vec3, from: u8, to: u8) -> bool {
        self.leaves.iter().enumerate().any(|(i, leaf)| {
            if !leaf.height_in_reach(feet) {
                return false;
            }
            let (a, b) = (self.rests[usize::from(from)].at[i], self.rests[usize::from(to)].at[i]);
            // A turn is sampled every hundredth of a radian, a slide every ten centimetres.
            let step = match leaf.motion {
                Motion::Hinge => 0.01,
                Motion::Rail(_) => 0.1,
            };
            let steps = ((b - a).abs() / step).ceil().max(1.0) as u32;
            (0..=steps).any(|step| leaf.over(feet, a + (b - a) * step as f32 / steps as f32, SWING_MARGIN))
        })
    }

    /// The joins that count for Active Zones: the Rest it stands at (`None` while it moves), if
    /// its gate Door (if any) is open.
    pub fn open_joins(&self, standing: Option<u8>, door_open: bool) -> &'static [(ZoneId, ZoneId)] {
        match standing {
            Some(rest) if door_open || self.door.is_none() => self.rests[usize::from(rest)].joins,
            _ => &[],
        }
    }

    /// The Levers, as (Zone, floor spot, facing). A Tide-driven Piece has none.
    pub fn levers(&self) -> &'static [(ZoneId, Vec3, Vec2)] {
        match self.driver {
            Driver::Levers(levers) => levers,
            Driver::Tide(_) | Driver::Quarantine(_) | Driver::Wheels(_) => &[],
        }
    }

    /// The Wheels of a Range. Any other Piece has none.
    pub fn wheels(&self) -> &'static [Wheel] {
        match self.driver {
            Driver::Wheels(wheels) => wheels,
            _ => &[],
        }
    }

    /// The Rest the Tide holds it at in `phase`, if the Tide drives it.
    pub fn tide_rest(&self, phase: TidePhase) -> Option<u8> {
        match self.driver {
            Driver::Tide(rests) => Some(rests[match phase {
                TidePhase::Low => 0,
                TidePhase::Rising => 1,
                TidePhase::High => 2,
                TidePhase::Falling => 3,
            }]),
            Driver::Levers(_) | Driver::Quarantine(_) | Driver::Wheels(_) => None,
        }
    }

    /// The Wing a Quarantine Door seals, if it is one.
    pub fn wing(&self) -> Option<WingId> {
        match self.driver {
            Driver::Quarantine(wing) => Some(wing),
            _ => None,
        }
    }
}

/// The Pieces of a Map, with their ids.
pub fn pieces(layout: &Layout) -> impl Iterator<Item = (PieceId, &Piece)> {
    layout.shifting.iter().enumerate().map(|(i, piece)| (PieceId(i as u8), piece))
}

/// Where a Piece is in its cycle.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PiecePhase {
    /// Standing still at a Rest, ready to be pulled.
    Rest(u8),
    /// The bell rings and the lamps burn; it still stands at `from`.
    Warning { from: u8, to: u8, elapsed: f32 },
    /// Waiting for everyone to clear the Leaves and their sweep.
    Waiting { from: u8, to: u8 },
    Swinging { from: u8, to: u8, elapsed: f32 },
    /// Standing at `at`, its Levers dead until the cooldown ends.
    Cooling { at: u8, elapsed: f32 },
}

/// What a step of a Piece's cycle did.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Beat {
    /// A Swing began, from the first Rest to the second.
    pub started: Option<(u8, u8)>,
    /// A Swing ended at this Rest.
    pub landed: Option<u8>,
}

impl PiecePhase {
    /// The Rest it stands at, or `None` while it moves.
    pub fn standing(self) -> Option<u8> {
        match self {
            PiecePhase::Rest(at) | PiecePhase::Cooling { at, .. } => Some(at),
            PiecePhase::Warning { from, .. } | PiecePhase::Waiting { from, .. } => Some(from),
            PiecePhase::Swinging { .. } => None,
        }
    }

    pub fn swinging(self) -> bool {
        matches!(self, PiecePhase::Swinging { .. })
    }

    /// Whether the Levers can be pulled.
    pub fn idle(self) -> bool {
        matches!(self, PiecePhase::Rest(_))
    }

    /// Whether the warning lamps burn: from the pull until it comes to Rest.
    pub fn lamps(self) -> bool {
        !self.idle()
    }

    /// Whether the bell rings.
    pub fn bell(self) -> bool {
        matches!(self, PiecePhase::Warning { .. } | PiecePhase::Waiting { .. })
    }

    /// The Rest it is going to, if it is on its way.
    pub fn heading(self) -> Option<(u8, u8)> {
        match self {
            PiecePhase::Warning { from, to, .. } | PiecePhase::Waiting { from, to } | PiecePhase::Swinging { from, to, .. } => Some((from, to)),
            _ => None,
        }
    }

    /// The pull: only a Piece at Rest starts.
    pub fn pull(&mut self, to: u8) {
        if let PiecePhase::Rest(from) = *self {
            *self = PiecePhase::Warning { from, to, elapsed: 0.0 };
        }
    }

    /// The debug Swing: straight to Swinging from wherever it stands, unless already moving.
    pub fn swing_now(&mut self, to: u8) -> Beat {
        match self.standing() {
            Some(from) => {
                *self = PiecePhase::Swinging { from, to, elapsed: 0.0 };
                Beat { started: Some((from, to)), landed: None }
            }
            None => Beat::default(),
        }
    }

    /// Advances `dt`. `clear` says nobody is in the way: Waiting only ever moves on with it.
    /// `cooldown` is how long its Levers or Wheels are dead after it lands.
    pub fn advance(&mut self, dt: f32, clear: bool, cooldown: f32) -> Beat {
        let mut beat = Beat::default();
        let mut left = dt;
        loop {
            match *self {
                PiecePhase::Rest(_) => return beat,
                PiecePhase::Warning { from, to, elapsed } => {
                    if elapsed + left < SWING_WARN_SECS {
                        *self = PiecePhase::Warning { from, to, elapsed: elapsed + left };
                        return beat;
                    }
                    left -= SWING_WARN_SECS - elapsed;
                    *self = PiecePhase::Waiting { from, to };
                }
                PiecePhase::Waiting { from, to } => {
                    if !clear {
                        return beat;
                    }
                    beat.started = Some((from, to));
                    *self = PiecePhase::Swinging { from, to, elapsed: 0.0 };
                }
                PiecePhase::Swinging { from, to, elapsed } => {
                    if elapsed + left < SWING_SECS {
                        *self = PiecePhase::Swinging { from, to, elapsed: elapsed + left };
                        return beat;
                    }
                    left -= SWING_SECS - elapsed;
                    beat.landed = Some(to);
                    *self = PiecePhase::Cooling { at: to, elapsed: 0.0 };
                }
                PiecePhase::Cooling { at, elapsed } => {
                    if elapsed + left < cooldown {
                        *self = PiecePhase::Cooling { at, elapsed: elapsed + left };
                        return beat;
                    }
                    left -= cooldown - elapsed;
                    *self = PiecePhase::Rest(at);
                }
            }
        }
    }

    /// The pose of each Leaf of `piece` now.
    pub fn poses(self, piece: &Piece) -> Vec<f32> {
        match self {
            PiecePhase::Swinging { from, to, elapsed } => piece.poses(from, to, elapsed),
            other => {
                let at = other.standing().unwrap_or(0);
                piece.rests[usize::from(at)].at.to_vec()
            }
        }
    }
}

/// What a Piece's Levers show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeverPrompt {
    /// Nothing: the system is Sabotaged.
    Dead,
    Swing,
    ClearTheBridge,
    Swinging,
    Settling,
}

impl LeverPrompt {
    pub fn of(phase: PiecePhase, sabotaged: bool) -> Self {
        if sabotaged {
            return LeverPrompt::Dead;
        }
        match phase {
            PiecePhase::Rest(_) => LeverPrompt::Swing,
            PiecePhase::Warning { .. } | PiecePhase::Waiting { .. } => LeverPrompt::ClearTheBridge,
            PiecePhase::Swinging { .. } => LeverPrompt::Swinging,
            PiecePhase::Cooling { .. } => LeverPrompt::Settling,
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            LeverPrompt::Dead => "",
            LeverPrompt::Swing => "Press F to swing the bridge",
            LeverPrompt::ClearTheBridge => "Clear the bridge",
            LeverPrompt::Swinging => "The bridge is swinging",
            LeverPrompt::Settling => "The bridge is settling",
        }
    }
}

/// What a Range's Wheel shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WheelPrompt {
    /// Nothing: the system is Sabotaged, or this Wheel's Aisle is the open one.
    None,
    Open,
    ClearTheAisle,
    Rolling,
    Settling,
}

impl WheelPrompt {
    /// What the Wheel that opens Rest `rest` shows while the Range is in `phase`. The Wheels of the Aisle
    /// that stands open show nothing, whatever the Range is doing.
    pub fn of(phase: PiecePhase, rest: u8, sabotaged: bool) -> Self {
        if sabotaged || phase.standing() == Some(rest) {
            return WheelPrompt::None;
        }
        match phase {
            PiecePhase::Rest(_) => WheelPrompt::Open,
            PiecePhase::Warning { .. } | PiecePhase::Waiting { .. } => WheelPrompt::ClearTheAisle,
            PiecePhase::Swinging { .. } => WheelPrompt::Rolling,
            PiecePhase::Cooling { .. } => WheelPrompt::Settling,
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            WheelPrompt::None => "",
            WheelPrompt::Open => "Press F to open this aisle",
            WheelPrompt::ClearTheAisle => "Clear the aisle",
            WheelPrompt::Rolling => "The stacks are rolling",
            WheelPrompt::Settling => "The stacks are settling",
        }
    }
}

/// The standing Rest of every Piece, by `PieceId`: what the planners and the Active Zones read.
pub fn standing_rests(pieces: impl IntoIterator<Item = (PieceId, PiecePhase)>, count: usize) -> Vec<Option<u8>> {
    let mut rests = vec![None; count];
    for (id, phase) in pieces {
        if let Some(slot) = rests.get_mut(usize::from(id.0)) {
            *slot = phase.standing();
        }
    }
    rests
}

/// A Piece in the world: its phase, which the Host decides and every machine runs on from.
#[derive(Component)]
pub struct PieceState {
    id: PieceId,
    phase: PiecePhase,
    /// The Player whose pull it was, whose Tipped kills it credits.
    activator: Option<Entity>,
    /// Whether the Tide or the Quarantine drives it: it comes to Rest at the end of a Swing, with no
    /// cooldown.
    driven: bool,
    /// How long its Levers or Wheels are dead after a Swing lands.
    cooldown: f32,
}

impl PieceState {
    pub fn id(&self) -> PieceId {
        self.id
    }

    pub fn phase(&self) -> PiecePhase {
        self.phase
    }

    /// Shows the phase the Host says it is in.
    pub fn show(&mut self, phase: PiecePhase) {
        self.phase = phase;
    }

    /// Runs the shown phase on until the Host says what comes next: it never leaves Waiting.
    pub fn advance_shown(&mut self, dt: f32) {
        self.phase.advance(dt, false, self.cooldown);
        self.settle();
    }

    /// A Tide-driven Piece has no cooldown: a landed Swing is at Rest.
    fn settle(&mut self) {
        if self.driven
            && let PiecePhase::Cooling { at, .. } = self.phase
        {
            self.phase = PiecePhase::Rest(at);
        }
    }

    /// A Piece standing at its start Rest.
    pub fn at_start(id: PieceId, piece: &Piece) -> Self {
        Self {
            id,
            phase: PiecePhase::Rest(piece.start),
            activator: None,
            driven: matches!(piece.driver, Driver::Tide(_) | Driver::Quarantine(_)),
            cooldown: piece.cooldown(),
        }
    }
}

/// One Leaf of a Piece, as a solid body.
#[derive(Component)]
pub struct LeafBody {
    pub piece: PieceId,
    pub index: usize,
}

/// One of a Piece's Levers, the Buyable that pulls it.
#[derive(Component)]
pub struct Lever {
    pub piece: PieceId,
    use_point: Vec3,
}

impl Lever {
    pub fn use_point(&self) -> Vec3 {
        self.use_point
    }
}

/// One of a Range's Wheels, the free Buyable that rolls it to open `rest`'s Aisle.
#[derive(Component)]
pub struct WheelPost {
    pub piece: PieceId,
    /// The Rest it rolls the Range to.
    pub rest: u8,
    use_point: Vec3,
}

impl WheelPost {
    pub fn use_point(&self) -> Vec3 {
        self.use_point
    }
}

/// The amber lamp on a Lever's post.
#[derive(Component)]
struct LeverLamp(PieceId);

/// The amber lamp on a Lock Gate's hinge post: lit through the bells and the Turn when the gate
/// is about to Swing.
#[derive(Component)]
struct GateLamp(PieceId);

/// A Zombie being Tipped into the canal: thrown off the side, then it drops and splashes.
#[derive(Component)]
pub struct Tipped {
    away: Vec3,
    age: f32,
}

/// A Swing began on this Piece, from one Rest to the other.
#[derive(Message, Debug, Clone, Copy)]
pub struct SwingStarted {
    pub piece: PieceId,
    pub from: u8,
    pub to: u8,
}

/// The collider of a Leaf: its deck and both rails, in the Leaf's own frame (the hinge at the
/// origin).
fn leaf_collider(leaf: &Leaf) -> Collider {
    if leaf.is_door() {
        // A door leaf is its slab: no rails.
        return Collider::compound(vec![(leaf.offset, Quat::IDENTITY, Collider::cuboid(leaf.size.x, leaf.size.y, leaf.size.z))]);
    }
    let rail = Vec3::new(LEAF_RAIL_THICKNESS, LEAF_RAIL_HEIGHT, leaf.size.z);
    let side = leaf.size.x / 2.0 - LEAF_RAIL_THICKNESS / 2.0;
    let up = leaf.offset.y + leaf.size.y / 2.0 + LEAF_RAIL_HEIGHT / 2.0;
    Collider::compound(vec![
        (leaf.offset, Quat::IDENTITY, Collider::cuboid(leaf.size.x, leaf.size.y, leaf.size.z)),
        (Vec3::new(leaf.offset.x - side, up, leaf.offset.z), Quat::IDENTITY, Collider::cuboid(rail.x, rail.y, rail.z)),
        (Vec3::new(leaf.offset.x + side, up, leaf.offset.z), Quat::IDENTITY, Collider::cuboid(rail.x, rail.y, rail.z)),
    ])
}

/// Spawns the Map's Pieces: every Leaf at its start Rest, the Levers and their lamps. Called by
/// `spawn_map`.
pub fn spawn_pieces(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, layout: &Layout) {
    if layout.shifting.is_empty() {
        return;
    }
    let timber = materials.add(Color::srgb(0.16, 0.12, 0.09));
    let iron = materials.add(Color::srgb(0.13, 0.14, 0.15));
    let lamp_off = materials.add(StandardMaterial {
        base_color: Color::srgb(0.4, 0.25, 0.05),
        emissive: LinearRgba::rgb(1.5, 0.8, 0.1),
        ..default()
    });
    let post = meshes.add(Cuboid::new(0.4, 1.0, 0.4));
    let arm = meshes.add(Cuboid::new(0.08, 0.6, 0.08));
    let lamp = meshes.add(Sphere::new(0.12));
    let rim = meshes.add(Torus::new(0.22, 0.28));
    // A Quarantine Door's Leaves are cream-green steel fire doors, drawn as the slab.
    let steel = materials.add(Color::srgb(0.66, 0.72, 0.6));
    for (id, piece) in pieces(layout) {
        commands.spawn((
            InRun,
            Name::new("Piece"),
            PieceState::at_start(id, piece),
            Transform::from_translation(piece.leaves[0].hinge),
            Visibility::default(),
        ));
        let poses = piece.rests[usize::from(piece.start)].at;
        for (index, leaf) in piece.leaves.iter().enumerate() {
            let deck = meshes.add(Cuboid::new(leaf.size.x, leaf.size.y, leaf.size.z));
            let rail = meshes.add(Cuboid::new(LEAF_RAIL_THICKNESS, LEAF_RAIL_HEIGHT, leaf.size.z));
            let side = leaf.size.x / 2.0 - LEAF_RAIL_THICKNESS / 2.0;
            let up = leaf.offset.y + leaf.size.y / 2.0 + LEAF_RAIL_HEIGHT / 2.0;
            commands
                .spawn((
                    InRun,
                    Name::new("Leaf"),
                    LeafBody { piece: id, index },
                    RigidBody::Kinematic,
                    leaf_collider(leaf),
                    CollisionLayers::new(GameLayer::World, LayerMask::ALL),
                    Transform::from_translation(leaf.origin(poses[index])).with_rotation(leaf.rotation(poses[index])),
                    Visibility::default(),
                ))
                .with_children(|parent| {
                    if leaf.is_door() {
                        if piece.wing().is_some() {
                            // A Quarantine Door's Leaf: painted steel with a vision panel, kick plate and signs.
                            crate::maps::quarantine_look::dress_leaf(parent, meshes, materials, leaf);
                        } else {
                            parent.spawn((Mesh3d(deck), MeshMaterial3d(steel.clone()), Transform::from_translation(leaf.offset)));
                        }
                        return;
                    }
                    parent.spawn((Mesh3d(deck), MeshMaterial3d(timber.clone()), Transform::from_translation(leaf.offset)));
                    for sign in [-1.0, 1.0] {
                        parent.spawn((
                            Mesh3d(rail.clone()),
                            MeshMaterial3d(iron.clone()),
                            Transform::from_xyz(leaf.offset.x + sign * side, up, leaf.offset.z),
                        ));
                    }
                });
        }
        if matches!(piece.driver, Driver::Tide(_)) {
            for leaf in piece.leaves {
                // On the rail, behind the hinge: away from where the Leaf lies when Shut.
                let shut = piece.rests[0].at[piece.leaves.iter().position(|other| other == leaf).unwrap_or(0)];
                let outward = (leaf.hinge - leaf.centre(shut)).with_y(0.0).normalize_or_zero();
                let side = Vec3::new(0.0, 0.0, 1.2);
                let at = leaf.hinge + outward * 0.25 + side;
                commands.spawn((InRun, Name::new("Gate post"), Mesh3d(post.clone()), MeshMaterial3d(iron.clone()), Transform::from_translation(at + Vec3::Y * 0.5)));
                commands.spawn((
                    InRun,
                    Name::new("Gate lamp"),
                    GateLamp(id),
                    Mesh3d(lamp.clone()),
                    MeshMaterial3d(lamp_off.clone()),
                    Transform::from_translation(at + Vec3::Y * 1.2),
                    Visibility::Hidden,
                ));
            }
        }
        for &(_, at, facing) in piece.levers() {
            let normal = Vec3::new(facing.x, 0.0, facing.y);
            let use_point = at + Vec3::Y * 1.1 + normal * 0.3;
            commands
                .spawn((
                    InRun,
                    Name::new("Lever"),
                    Lever { piece: id, use_point },
                    Mesh3d(post.clone()),
                    MeshMaterial3d(iron.clone()),
                    Transform::from_translation(at + Vec3::Y * 0.5).looking_to(-normal, Vec3::Y),
                    Visibility::default(),
                ))
                .with_children(|parent| {
                    parent.spawn((Mesh3d(arm.clone()), MeshMaterial3d(timber.clone()), Transform::from_xyz(0.0, 0.7, 0.15).with_rotation(Quat::from_rotation_x(-0.5))));
                    parent.spawn((LeverLamp(id), Mesh3d(lamp.clone()), MeshMaterial3d(lamp_off.clone()), Transform::from_xyz(0.0, 0.62, 0.0), Visibility::Hidden));
                });
        }
        // A Range's Wheels: a cast-iron post with a spoked hand wheel, no collider (the Aisle's mouth is
        // clear), shown only where this Aisle is shut.
        for wheel in piece.wheels() {
            let normal = Vec3::new(wheel.facing.x, 0.0, wheel.facing.y);
            let use_point = wheel.at + Vec3::Y * 1.1 + normal * 0.3;
            commands
                .spawn((
                    InRun,
                    Name::new("Wheel"),
                    WheelPost { piece: id, rest: wheel.rest, use_point },
                    Mesh3d(post.clone()),
                    MeshMaterial3d(iron.clone()),
                    Transform::from_translation(wheel.at + Vec3::Y * 0.5).looking_to(-normal, Vec3::Y),
                    Visibility::default(),
                ))
                .with_children(|parent| {
                    parent.spawn((Mesh3d(rim.clone()), MeshMaterial3d(iron.clone()), Transform::from_xyz(0.0, 0.62, 0.2).with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))));
                    parent.spawn((Mesh3d(arm.clone()), MeshMaterial3d(timber.clone()), Transform::from_xyz(0.0, 0.62, 0.2).with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2))));
                });
        }
    }
}

pub struct ShiftingPlugin;

impl Plugin for ShiftingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::maps::quarantine::QuarantinePlugin)
            .add_message::<SwingStarted>()
            .add_message::<SwingNearest>()
            .add_message::<Teleport>()
            .add_systems(FixedUpdate, (pose_leaves, throw_tipped).run_if(world_runs))
            .add_systems(FixedUpdate, (move_sappers, (shove_swinging, move_swept).chain()).in_set(HostSimulation).run_if(world_runs))
            .add_systems(
                Update,
                (
                    (
                        use_levers,
                        use_wheels,
                        swing_nearest,
                        drive_gates.after(TideTurn),
                        drive_quarantine.after(QuarantineClock),
                        run_pieces,
                        tip_occupants,
                        wash_gate_players,
                        sweep_quarantine,
                        shove_range,
                    )
                        .chain()
                        .in_set(HostSimulation),
                    (prompt_levers, prompt_wheels, show_wheels, light_lamps, light_gate_lamps, ring_cues).chain(),
                )
                    .run_if(world_runs),
            );
    }
}

/// Poses every Leaf from its Piece's phase, in the fixed step so the colliders move with physics.
fn pose_leaves(pieces: Query<&PieceState>, mut leaves: Query<(&LeafBody, &mut Transform)>, map: Res<MapId>) {
    let layout = map.layout();
    for (body, mut transform) in &mut leaves {
        let Some(state) = pieces.iter().find(|state| state.id == body.piece) else { continue };
        let Some(piece) = layout.shifting.get(usize::from(body.piece.0)) else { continue };
        let Some(leaf) = piece.leaves.get(body.index) else { continue };
        let pose = state.phase.poses(piece)[body.index];
        let (origin, rotation) = (leaf.origin(pose), leaf.rotation(pose));
        if transform.rotation != rotation {
            transform.rotation = rotation;
        }
        if transform.translation != origin {
            transform.translation = origin;
        }
    }
}

/// A bought Lever pulls its Piece to the next Rest, unless it's Sabotaged or busy.
fn use_levers(
    mut purchases: MessageReader<Purchased>,
    levers: Query<&Lever>,
    mut pieces: Query<(&mut PieceState, Has<Sabotaged>)>,
    map: Res<MapId>,
) {
    for &Purchased { player, buyable } in purchases.read() {
        let Ok(lever) = levers.get(buyable) else { continue };
        let Some(piece) = map.layout().shifting.get(usize::from(lever.piece.0)) else { continue };
        for (mut state, sabotaged) in &mut pieces {
            if state.id == lever.piece && !sabotaged && state.phase.idle() {
                let to = piece.next_rest(state.phase.standing().unwrap_or(0));
                state.phase.pull(to);
                state.activator = Some(player);
            }
        }
    }
}

/// A bought (used) Wheel rolls its Range to open its own Aisle, if the Range is idle, the Aisle is
/// shut and it isn't Sabotaged. The Host decides: a client's use arrives as the same `Purchased`.
fn use_wheels(
    mut purchases: MessageReader<Purchased>,
    wheels: Query<&WheelPost>,
    mut pieces: Query<(&mut PieceState, Has<Sabotaged>)>,
    map: Res<MapId>,
) {
    for &Purchased { player, buyable } in purchases.read() {
        let Ok(post) = wheels.get(buyable) else { continue };
        let Some(piece) = map.layout().shifting.get(usize::from(post.piece.0)) else { continue };
        let Some(wheel) = piece.wheels().iter().find(|wheel| wheel.rest == post.rest) else { continue };
        for (mut state, sabotaged) in &mut pieces {
            if state.id != post.piece || sabotaged {
                continue;
            }
            if let Some(to) = wheel.opens(state.phase) {
                state.phase.pull(to);
                state.activator = Some(player);
            }
        }
    }
}

/// Runs each Piece's cycle. A Swing starts only once no Player, Down or not, is on a Leaf or in
/// its sweep.
fn run_pieces(
    mut pieces: Query<&mut PieceState>,
    players: Players,
    map: Res<MapId>,
    time: Res<Time>,
    mut swings: MessageWriter<SwingStarted>,
) {
    let layout = map.layout();
    for mut state in &mut pieces {
        let Some(piece) = layout.shifting.get(usize::from(state.id.0)) else { continue };
        // The warning runs into the wait in one step, so Warning checks the way too.
        let clear = match state.phase.heading() {
            Some((from, to)) if !state.phase.swinging() => !players.iter().any(|player| piece.in_way(player.feet, from, to)),
            _ => true,
        };
        let id = state.id;
        let beat = state.phase.advance(time.delta_secs(), clear, piece.cooldown());
        state.settle();
        if let Some((from, to)) = beat.started {
            swings.write(SwingStarted { piece: id, from, to });
        }
    }
}

/// Request to swing the Lever-driven Piece nearest `at` to its next Rest now: no price, warning or
/// cooldown, and it still Tips. The Host judges it.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct SwingNearest {
    pub at: Vec3,
}

/// A `SwingNearest` swings the Piece nearest it now.
fn swing_nearest(
    mut asked: MessageReader<SwingNearest>,
    mut pieces: Query<&mut PieceState>,
    map: Res<MapId>,
    mut swings: MessageWriter<SwingStarted>,
) {
    let Some(&SwingNearest { at }) = asked.read().last() else { return };
    let layout = map.layout();
    let nearest = pieces
        .iter()
        .filter_map(|state| layout.shifting.get(usize::from(state.id.0)).filter(|piece| matches!(piece.driver, Driver::Levers(_))).map(|piece| (state.id, piece.leaves[0].hinge.distance(at))))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id);
    for mut state in &mut pieces {
        if Some(state.id) != nearest {
            continue;
        }
        let Some(piece) = layout.shifting.get(usize::from(state.id.0)) else { continue };
        let next = piece.next_rest(state.phase.standing().unwrap_or(0));
        if let Some(from) = state.phase.standing() {
            let id = state.id;
            state.phase.swing_now(next);
            swings.write(SwingStarted { piece: id, from, to: next });
        }
    }
}

/// The Tide drives its Pieces: a Turn whose new phase has a different Rest sets the Piece
/// Swinging from the start of Turning, over the Turn's seconds, and the Tide landing puts it at the
/// Rest of the phase it lands on (a Turn finished at once swings nothing: the Swing's effects come
/// at once). It has no Lever, no warning and no wait: the bells were the warning.
fn drive_gates(tide: Option<Res<Tide>>, mut pieces: Query<&mut PieceState>, map: Res<MapId>, mut swings: MessageWriter<SwingStarted>) {
    let Some(tide) = tide else { return };
    let layout = map.layout();
    for mut state in &mut pieces {
        let Some(piece) = layout.shifting.get(usize::from(state.id.0)) else { continue };
        let Some(now) = piece.tide_rest(tide.phase) else { continue };
        let id = state.id;
        match tide.turn {
            TurnState::Turning { elapsed } => {
                let Some(to) = piece.tide_rest(tide.phase.next()) else { continue };
                if to != now && state.phase == PiecePhase::Rest(now) {
                    state.phase = PiecePhase::Swinging { from: now, to, elapsed };
                    swings.write(SwingStarted { piece: id, from: now, to });
                }
            }
            TurnState::Still => match state.phase {
                PiecePhase::Rest(at) if at == now => {}
                PiecePhase::Rest(at) => {
                    // The Turn was finished at once: the Swing's effects come as it is put at Rest.
                    state.phase = PiecePhase::Rest(now);
                    swings.write(SwingStarted { piece: id, from: at, to: now });
                }
                _ => state.phase = PiecePhase::Rest(now),
            },
            TurnState::Bells { .. } => {}
        }
    }
}

/// A Player on a Lock Gate's top or in its sweep when its Swing starts is Washed Up to the nearer
/// landing node of the Rest they were on, with no damage and no Down (a Down Player keeps their
/// Last Stand). The Tide can't wait: the bells were the warning.
fn wash_gate_players(
    mut swings: MessageReader<SwingStarted>,
    mut players: Query<(Entity, &mut Transform, Option<&Locomotion>), With<Player>>,
    graph: Option<Res<crate::enemies::nav::NavGraph>>,
    map: Res<MapId>,
    mut teleport: MessageWriter<Teleport>,
) {
    let layout = map.layout();
    for &SwingStarted { piece: id, from, to } in swings.read() {
        let Some(piece) = layout.shifting.get(usize::from(id.0)) else { continue };
        if !matches!(piece.driver, Driver::Tide(_)) {
            continue;
        }
        let Some(graph) = graph.as_ref().filter(|graph| graph.map == *map) else { continue };
        let landings: Vec<Vec3> = graph
            .edges
            .iter()
            .filter(|edge| edge.gate == crate::enemies::nav::Gate::Rest(id, from))
            .flat_map(|edge| [graph.nodes[edge.from].at, graph.nodes[edge.to].at])
            .collect();
        for (player, mut transform, locomotion) in &mut players {
            let feet = transform.translation - Vec3::Y * half_height_above_feet(locomotion);
            if !piece.in_way(feet, from, to) {
                continue;
            }
            let Some(&landing) = landings.iter().min_by(|a, b| a.distance(feet).total_cmp(&b.distance(feet))) else { continue };
            transform.translation = landing + Vec3::Y * (Stance::Standing.half_height() + 0.05);
            teleport.write(Teleport { player, feet: landing, facing: None });
        }
    }
}

/// The Quarantine drives its Pieces: a Wing's doors Swing shut as it Seals and open as it Opens, and
/// otherwise stand where the stage says (shut through Sealed and Release, Open at every other time and
/// for every other Wing). A Swing that is under way lands before anything else is decided. Both
/// Swings come with the sweep's effects at once: nothing waits for anyone to clear.
fn drive_quarantine(quarantine: Res<Quarantine>, mut pieces: Query<&mut PieceState>, map: Res<MapId>, mut swings: MessageWriter<SwingStarted>) {
    let layout = map.layout();
    for mut state in &mut pieces {
        let Some(piece) = layout.shifting.get(usize::from(state.id.0)) else { continue };
        let Some(wing) = piece.wing() else { continue };
        let Some(at) = state.phase.standing() else { continue };
        let seal = quarantine.seals(wing);
        let to = u8::from(seal);
        if at == to {
            continue;
        }
        // Over its own Swing's seconds, as far into it as the stage is.
        let elapsed = if matches!(quarantine.stage(), Stage::Sealing | Stage::Opening) { quarantine.elapsed() } else { 0.0 };
        let id = state.id;
        state.phase = PiecePhase::Swinging { from: at, to, elapsed };
        swings.write(SwingStarted { piece: id, from: at, to });
    }
}

/// Where a Quarantine Door's outside landing is: the node outside the Wing that the doorway's Door edge
/// leads to. `None` on a graph for another Map, or one with no such edge.
pub fn outside_landing(graph: &crate::enemies::nav::NavGraph, id: PieceId, piece: &Piece) -> Option<Vec3> {
    use crate::enemies::nav::Gate;
    let door = piece.door?;
    let doorway = graph.edges.iter().find(|edge| edge.gate == Gate::Rest(id, 0))?.from;
    let edge = graph.edges.iter().find(|edge| {
        matches!(edge.gate, Gate::Door(a, b) if (a, b) == door || (b, a) == door) && (edge.from == doorway || edge.to == doorway)
    })?;
    let landing = if edge.from == doorway { edge.to } else { edge.from };
    Some(graph.nodes[landing].at)
}

/// A Zombie, Hellhound, special or Elite a Quarantine Door's Swing has caught, or a Range has Shoved,
/// to be put at `to`.
#[derive(Component)]
pub struct Swept {
    to: Vec3,
}

/// When a Quarantine Door starts to Swing, either way, every Player (Down or not) and every enemy on
/// a Leaf or in its sweep is put at the doorway's outside landing. Nothing is hurt: a Player takes no
/// damage and is not Downed, and a Down one keeps their Last Stand; an enemy is not killed, but plans
/// its route again from where it lands.
fn sweep_quarantine(
    mut commands: Commands,
    mut swings: MessageReader<SwingStarted>,
    mut players: Query<(Entity, &mut Transform, Option<&Locomotion>), With<Player>>,
    zombies: Query<(Entity, &Zombie, &Transform), (Without<Dying>, Without<Player>)>,
    graph: Option<Res<crate::enemies::nav::NavGraph>>,
    map: Res<MapId>,
    mut teleport: MessageWriter<Teleport>,
) {
    let layout = map.layout();
    for &SwingStarted { piece: id, from, to } in swings.read() {
        let Some(piece) = layout.shifting.get(usize::from(id.0)) else { continue };
        if piece.wing().is_none() {
            continue;
        }
        let Some(landing) = graph.as_ref().filter(|graph| graph.map == *map).and_then(|graph| outside_landing(graph, id, piece)) else { continue };
        for (player, mut transform, locomotion) in &mut players {
            let feet = transform.translation - Vec3::Y * half_height_above_feet(locomotion);
            if !piece.in_way(feet, from, to) {
                continue;
            }
            transform.translation = landing + Vec3::Y * (Stance::Standing.half_height() + 0.05);
            teleport.write(Teleport { player, feet: landing, facing: None });
        }
        for (entity, zombie, transform) in &zombies {
            let feet = transform.translation - Vec3::Y * zombie.half_height();
            if piece.in_way(feet, from, to) {
                commands.entity(entity).insert(Swept { to: landing });
            }
        }
    }
}

/// The two mouth nodes of the Aisle that is open at Rest `rest` of Piece `id`: the ends of its chain
/// (mouth, Aisle, mouth) of `Gate::Rest` edges, which are the nodes that edges meet only once.
pub fn mouths(graph: &crate::enemies::nav::NavGraph, id: PieceId, rest: u8) -> Vec<Vec3> {
    let gate = crate::enemies::nav::Gate::Rest(id, rest);
    let ends: Vec<usize> = graph.edges.iter().filter(|edge| edge.gate == gate).flat_map(|edge| [edge.from, edge.to]).collect();
    ends.iter().filter(|&&node| ends.iter().filter(|&&other| other == node).count() == 1).map(|&node| graph.nodes[node].at).collect()
}

/// The nearest mouth node of the Aisles open at `rests` to `feet`. `None` on a graph with no such Aisle.
pub fn shove_landing(graph: &crate::enemies::nav::NavGraph, id: PieceId, rests: &[u8], feet: Vec3) -> Option<Vec3> {
    rests.iter().flat_map(|&rest| mouths(graph, id, rest)).min_by(|a, b| a.distance(feet).total_cmp(&b.distance(feet)))
}

/// Where a Range Shoves a body standing at `feet` when its Roll starts from Rest `from`: the nearer mouth
/// of the Aisle that is closing, which is where the body stands. A body in the margin that the sweep
/// takes beyond the Bookcases' ends, away from that Aisle's column, is not in the Aisle: it is moved to
/// the nearest mouth of any Aisle of the Range, which is a step across the strip.
pub fn shove_landing_at_start(graph: &crate::enemies::nav::NavGraph, piece: &Piece, id: PieceId, from: u8, feet: Vec3) -> Option<Vec3> {
    let own = shove_landing(graph, id, &[from], feet)?;
    let along = match piece.leaves.first().map(|leaf| leaf.motion) {
        Some(Motion::Rail(rail)) => Vec2::new(feet.x - own.x, feet.z - own.z).dot(rail).abs(),
        _ => 0.0,
    };
    if along <= AISLE_WIDTH / 2.0 + SWING_MARGIN {
        return Some(own);
    }
    let every: Vec<u8> = (0..piece.rests.len() as u8).collect();
    shove_landing(graph, id, &every, feet)
}

/// When a Range's Roll starts, every Player (Down or not) and every Zombie, Hellhound, special and
/// Elite in its sweep is Shoved to the nearer mouth of the Aisle it was in, unharmed: no damage, no
/// Down, no death effect (a Napalm doesn't burst), no kill. A Player is moved by `Teleport`, an enemy as
/// a clean cut that plans its route again. Nothing is ever crushed.
fn shove_range(
    mut commands: Commands,
    mut swings: MessageReader<SwingStarted>,
    mut players: Query<(Entity, &mut Transform, Option<&Locomotion>), With<Player>>,
    zombies: Query<(Entity, &Zombie, &Transform), (Without<Dying>, Without<Player>)>,
    graph: Option<Res<crate::enemies::nav::NavGraph>>,
    map: Res<MapId>,
    mut teleport: MessageWriter<Teleport>,
) {
    let layout = map.layout();
    for &SwingStarted { piece: id, from, to } in swings.read() {
        let Some(piece) = layout.shifting.get(usize::from(id.0)) else { continue };
        if !matches!(piece.driver, Driver::Wheels(_)) {
            continue;
        }
        let Some(graph) = graph.as_ref().filter(|graph| graph.map == *map) else { continue };
        for (player, mut transform, locomotion) in &mut players {
            let feet = transform.translation - Vec3::Y * half_height_above_feet(locomotion);
            if !piece.in_way(feet, from, to) {
                continue;
            }
            let Some(landing) = shove_landing_at_start(graph, piece, id, from, feet) else { continue };
            transform.translation = landing + Vec3::Y * (Stance::Standing.half_height() + 0.05);
            teleport.write(Teleport { player, feet: landing, facing: None });
        }
        for (entity, zombie, transform) in &zombies {
            let feet = transform.translation - Vec3::Y * zombie.half_height();
            if !piece.in_way(feet, from, to) {
                continue;
            }
            if let Some(landing) = shove_landing_at_start(graph, piece, id, from, feet) {
                commands.entity(entity).insert(Swept { to: landing });
            }
        }
    }
}

/// Each fixed tick a Range is Rolling, any Player or Zombie whose feet are in its sweep (they stepped
/// in after the Roll started) is Shoved the same way, to the nearest mouth of the Range: they came in
/// from a mouth strip or an Aisle's end, so that is where they are put back, and no body is ever between
/// two Bookcases as they meet.
fn shove_swinging(
    mut commands: Commands,
    pieces: Query<&PieceState>,
    mut players: Query<(Entity, &mut Transform, Option<&Locomotion>), With<Player>>,
    zombies: Query<(Entity, &Zombie, &Transform), (Without<Dying>, Without<Player>, Without<Swept>)>,
    graph: Option<Res<crate::enemies::nav::NavGraph>>,
    map: Res<MapId>,
    mut teleport: MessageWriter<Teleport>,
) {
    let layout = map.layout();
    let Some(graph) = graph.as_ref().filter(|graph| graph.map == *map) else { return };
    for state in &pieces {
        let PiecePhase::Swinging { from, to, .. } = state.phase else { continue };
        let Some(piece) = layout.shifting.get(usize::from(state.id.0)) else { continue };
        if !matches!(piece.driver, Driver::Wheels(_)) {
            continue;
        }
        let every: Vec<u8> = (0..piece.rests.len() as u8).collect();
        for (player, mut transform, locomotion) in &mut players {
            let feet = transform.translation - Vec3::Y * half_height_above_feet(locomotion);
            if !piece.in_way(feet, from, to) {
                continue;
            }
            let Some(landing) = shove_landing(graph, state.id, &every, feet) else { continue };
            transform.translation = landing + Vec3::Y * (Stance::Standing.half_height() + 0.05);
            teleport.write(Teleport { player, feet: landing, facing: None });
        }
        for (entity, zombie, transform) in &zombies {
            let feet = transform.translation - Vec3::Y * zombie.half_height();
            if !piece.in_way(feet, from, to) {
                continue;
            }
            if let Some(landing) = shove_landing(graph, state.id, &every, feet) {
                commands.entity(entity).insert(Swept { to: landing });
            }
        }
    }
}

/// An enemy a Swing caught is put on the outside landing as a clean cut, and plans again.
fn move_swept(
    mut commands: Commands,
    mut swept: Query<(Entity, &Zombie, &Swept, &mut Transform, &mut TranslationEasingState, &mut Route), (Without<Dying>, Without<Player>)>,
) {
    for (entity, zombie, swept, mut transform, mut easing, mut route) in &mut swept {
        commands.entity(entity).remove::<Swept>();
        transform.translation = swept.to + Vec3::Y * (zombie.half_height() + 0.02);
        easing.start = Some(transform.translation);
        *route = Route::default();
    }
}

/// When a Swing starts, every Zombie and Hellhound on a Leaf or in its sweep is Tipped: it dies
/// at once as `Attack::Swing`, an Unpaid Kill. A Sapper loses the Trap's share of its health
/// and is moved to a fresh spawn point instead.
fn tip_occupants(
    mut commands: Commands,
    mut swings: MessageReader<SwingStarted>,
    zombies: Query<(Entity, &Zombie, &Transform), (Without<Dying>, Without<Rising>)>,
    pieces: Query<&PieceState>,
    mut damage: MessageWriter<DamageZombie>,
    map: Res<MapId>,
) {
    let layout = map.layout();
    for &SwingStarted { piece: id, from, to } in swings.read() {
        let Some(piece) = layout.shifting.get(usize::from(id.0)) else { continue };
        // A Quarantine Door is no Trap: it puts bodies outside, it kills none. Nor is a Range: it
        // never crushes or Tips.
        if matches!(piece.driver, Driver::Quarantine(_) | Driver::Wheels(_)) {
            continue;
        }
        let activator = pieces.iter().find(|state| state.id == id).and_then(|state| state.activator);
        for (entity, zombie, transform) in &zombies {
            let feet = transform.translation - Vec3::Y * zombie.half_height();
            if !piece.in_way(feet, from, to) {
                continue;
            }
            if zombie.is_sapper() {
                // Not killed: it loses the Trap's share and is moved (see `move_sappers`).
                damage.write(DamageZombie {
                    player: activator,
                    zombie: entity,
                    damage: zombie.max_health() * TRAP_SAPPER_HEALTH_FRACTION,
                    zone: HitZone::Torso,
                    attack: Attack::Swing,
                });
                commands.entity(entity).insert(Tossed);
                continue;
            }
            // Off the side of the deck, away from its hinge line.
            let hinge = piece.leaves[0].hinge;
            let away = (feet - hinge).with_y(0.0).normalize_or_zero();
            commands.entity(entity).insert(Tipped { away, age: 0.0 });
            damage.write(DamageZombie {
                player: activator,
                zombie: entity,
                damage: f32::INFINITY,
                zone: HitZone::Torso,
                attack: Attack::Swing,
            });
        }
    }
}

/// A Sapper a Swing caught: to be moved to a fresh spawn point.
#[derive(Component)]
pub struct Tossed;

/// A Tipped body is thrown off the side and drops into the canal, where it splashes and is gone.
fn throw_tipped(
    mut commands: Commands,
    mut bodies: Query<(Entity, &mut Tipped, &mut Transform), With<Dying>>,
    time: Res<Time>,
    mut cues: MessageWriter<PlayCue>,
) {
    let dt = time.delta_secs();
    for (entity, mut tipped, mut transform) in &mut bodies {
        tipped.age += dt;
        let step = tipped.away * TIPPED_THROW_SPEED * dt;
        transform.translation += step;
        transform.translation.y -= dt * 1.7 / TIPPED_FALL_SECS;
        if tipped.age >= TIPPED_FALL_SECS {
            cues.write(PlayCue { cue: Cue::Splash, at: Some(transform.translation), pitch: 0.9, gain: 1.0 });
            commands.entity(entity).despawn();
        }
    }
}

/// A Sapper a Swing caught goes to a fresh Special Spawn Point, as one Out of Bounds does, and
/// goes on with what it was doing. Moved in the fixed step as a clean cut.
fn move_sappers(
    mut commands: Commands,
    mut sappers: Query<(Entity, &mut Transform, &mut TranslationEasingState, &mut Route), (With<Tossed>, Without<Dying>, Without<Player>)>,
    spawn_points: Query<(&Transform, &SpawnPoint), (Without<Tossed>, Without<Player>)>,
    players: Players,
    active: Res<ActiveZones>,
) {
    for (entity, mut transform, mut easing, mut route) in &mut sappers {
        commands.entity(entity).remove::<Tossed>();
        let points = special_spawn_candidates(
            spawn_points.iter().filter(|(_, point)| point.kind == SpawnKind::Special).map(|(t, point)| (point.zone, t.translation)),
            &players.spawn_anchors().iter().map(|spot| (spot.zone, spot.centre)).collect::<Vec<_>>(),
            active.0,
        );
        if points.is_empty() {
            continue;
        }
        let at = points[rand::random_range(0..points.len())];
        transform.translation = at + Vec3::Y * (ZOMBIE_RADIUS + ZOMBIE_CAPSULE_LENGTH / 2.0 + 0.02);
        easing.start = Some(transform.translation);
        *route = Route::default();
    }
}

/// Levers own their Buyables: the pull with its price, or why it can't be pulled.
fn prompt_levers(
    mut commands: Commands,
    levers: Query<(Entity, &Lever, Option<&Buyable>, Has<Refused>)>,
    pieces: Query<(&PieceState, Has<Sabotaged>)>,
) {
    for (entity, lever, buyable, refused) in &levers {
        let Some((state, sabotaged)) = pieces.iter().find(|(state, _)| state.id == lever.piece) else { continue };
        let want = LeverPrompt::of(state.phase, sabotaged);
        let (text, price) = match want {
            LeverPrompt::Swing => (want.text(), Some(LEVER_PRICE)),
            _ => (want.text(), None),
        };
        let current = buyable.map(|buyable| (buyable.prompt.as_str(), buyable.price));
        if want == LeverPrompt::Dead {
            if buyable.is_some() || refused {
                commands.entity(entity).remove::<(Buyable, Refused)>();
            }
            continue;
        }
        if current == Some((text, price)) && refused == (price.is_none()) {
            continue;
        }
        let mut lever_entity = commands.entity(entity);
        lever_entity.insert(Buyable { use_point: lever.use_point, prompt: text.into(), price, range: USE_RANGE });
        if price.is_none() {
            lever_entity.insert(Refused);
        } else {
            lever_entity.remove::<Refused>();
        }
    }
}

/// Wheels own their Buyables: a free use where the Range is idle and this Aisle is shut, or why it
/// can't be used, and nothing at the Aisle that is open.
fn prompt_wheels(
    mut commands: Commands,
    wheels: Query<(Entity, &WheelPost, Option<&Buyable>, Has<Refused>)>,
    pieces: Query<(&PieceState, Has<Sabotaged>)>,
) {
    for (entity, wheel, buyable, refused) in &wheels {
        let Some((state, sabotaged)) = pieces.iter().find(|(state, _)| state.id == wheel.piece) else { continue };
        let want = WheelPrompt::of(state.phase, wheel.rest, sabotaged);
        if want == WheelPrompt::None {
            if buyable.is_some() || refused {
                commands.entity(entity).remove::<(Buyable, Refused)>();
            }
            continue;
        }
        let text = want.text();
        let usable = want == WheelPrompt::Open;
        if buyable.is_some_and(|buyable| buyable.prompt == text) && refused == !usable {
            continue;
        }
        let mut wheel_entity = commands.entity(entity);
        wheel_entity.insert(Buyable { use_point: wheel.use_point, prompt: text.into(), price: None, range: USE_RANGE });
        if usable {
            wheel_entity.remove::<Refused>();
        } else {
            wheel_entity.insert(Refused);
        }
    }
}

/// A Wheel's post stands only where its Aisle is shut: the open Aisle's is gone from its mouth.
fn show_wheels(pieces: Query<&PieceState>, mut wheels: Query<(&WheelPost, &mut Visibility)>) {
    for (wheel, mut visibility) in &mut wheels {
        let open = pieces.iter().find(|state| state.id == wheel.piece).is_some_and(|state| state.phase.standing() == Some(wheel.rest));
        visibility.set_if_neq(if open { Visibility::Hidden } else { Visibility::Inherited });
    }
}

/// The amber lamps burn from the pull until the Piece is back at Rest.
fn light_lamps(pieces: Query<&PieceState>, mut lamps: Query<(&LeverLamp, &mut Visibility)>) {
    for (lamp, mut visibility) in &mut lamps {
        let lit = pieces.iter().find(|state| state.id == lamp.0).is_some_and(|state| state.phase.lamps());
        let shown = if lit { Visibility::Inherited } else { Visibility::Hidden };
        visibility.set_if_neq(shown);
    }
}

/// The amber lamps on a Lock Gate burn through the bells and the Turn when it will Swing, and
/// while it does.
fn light_gate_lamps(tide: Option<Res<Tide>>, pieces: Query<&PieceState>, map: Res<MapId>, mut lamps: Query<(&GateLamp, &mut Visibility)>) {
    let layout = map.layout();
    for (lamp, mut visibility) in &mut lamps {
        let lit = pieces.iter().find(|state| state.id == lamp.0).is_some_and(|state| {
            let moving = layout.shifting.get(usize::from(state.id.0)).zip(tide.as_ref()).is_some_and(|(piece, tide)| {
                tide.turning() && piece.tide_rest(tide.phase) != piece.tide_rest(tide.phase.next())
            });
            moving || state.phase.swinging()
        });
        let shown = if lit { Visibility::Inherited } else { Visibility::Hidden };
        visibility.set_if_neq(shown);
    }
}

/// The clunk at Rest, on every machine, from the phase it shows; a Quarantine Door instead sounds as
/// it starts to Swing and slams as it lands shut.
fn ring_cues(
    pieces: Query<&PieceState>,
    map: Res<MapId>,
    mut was: Local<Vec<(PieceId, bool)>>,
    mut cues: MessageWriter<PlayCue>,
) {
    for state in &pieces {
        let swinging = state.phase.swinging();
        let before = was.iter().position(|(id, _)| *id == state.id);
        let had = before.map(|i| was[i].1).unwrap_or(false);
        match before {
            Some(i) => was[i].1 = swinging,
            None => was.push((state.id, swinging)),
        }
        if had == swinging {
            continue;
        }
        let Some(piece) = map.layout().shifting.get(usize::from(state.id.0)) else { continue };
        let poses = state.phase.poses(piece);
        let at = piece.leaves[0].centre(poses[0]);
        if piece.wing().is_some() {
            if swinging {
                cues.write(PlayCue::new(Cue::QuarantineSwing, Some(door_centre(piece))));
            } else if state.phase.standing() == Some(1) {
                cues.write(PlayCue::new(Cue::QuarantineSlam, Some(door_centre(piece))));
            }
        } else if !swinging && !matches!(piece.driver, Driver::Wheels(_)) {
            // (A Range's Bookcases thud, and its Wheels creak, in `audio::range`.)
            cues.write(PlayCue::new(Cue::BridgeClunk, Some(at)));
        }
    }
}

/// The middle of a Quarantine Door's doorway, at head height: between its two hinges.
pub fn door_centre(piece: &Piece) -> Vec3 {
    piece.leaves.iter().map(|leaf| leaf.hinge).sum::<Vec3>() / piece.leaves.len() as f32 + Vec3::Y * 1.2
}

/// A Door that is the gate of a Piece, if it is open.
pub fn gate_open(doors: &Query<&Door>, gate: Option<(ZoneId, ZoneId)>) -> bool {
    gate.is_none_or(|gate| doors.iter().any(|door| door.open && door.gate.is_some_and(|key| key == gate || (key.1, key.0) == gate)))
}

/// The joins every Piece's standing Rest makes open, for Active Zones.
pub fn open_joins(layout: &Layout, rests: &[Option<u8>], doors: &Query<&Door>) -> Vec<(ZoneId, ZoneId)> {
    pieces(layout)
        .flat_map(|(id, piece)| {
            let standing = rests.get(usize::from(id.0)).copied().flatten();
            piece.open_joins(standing, gate_open(doors, piece.door)).iter().copied()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use bevy::ecs::system::RunSystemOnce;

    use super::*;
    use crate::economy::buy::Purchased;
    use crate::enemies::nav::{Gate, NavGraph, NavState};
    use crate::enemies::zombie::headless::Sim;
    use crate::enemies::special::SpecialKind;
    use crate::enemies::zombie::{Heavyweight, Pay, ZombieKilled};
    use crate::maps::tide::TIDE_PHASES;
    use crate::maps::water::{Wading, water_at};
    use crate::maps::zone::{ActiveZones, PlayerZone, update_active_zones};
    use ZoneId::*;

    fn polder() -> &'static Layout {
        MapId::Polder.layout()
    }

    /// The Market Bridge: Span (0) joins the Square and the Lock Yard, Along (1) nothing.
    fn market() -> &'static Piece {
        &polder().shifting[0]
    }

    fn all_doors() -> Vec<(ZoneId, ZoneId)> {
        let zones = polder().zones;
        zones.iter().flat_map(|&a| zones.iter().map(move |&b| (a, b))).collect()
    }

    // ---- Pure ----

    #[test]
    fn a_swing_ends_exactly_on_each_rests_yaw() {
        for (from, to) in [(0.0, FRAC_PI_2), (FRAC_PI_2, 0.0), (0.0, -FRAC_PI_2), (-FRAC_PI_2, 0.0)] {
            assert_eq!(pose(from, to, 0.0), from);
            assert_eq!(pose(from, to, SWING_SECS), to);
            assert_eq!(pose(from, to, SWING_SECS + 3.0), to);
            let half = pose(from, to, SWING_SECS / 2.0);
            assert!((half - (from + to) / 2.0).abs() < 1e-5, "smoothstep is halfway at the middle");
        }
        assert_eq!((SWING_WARN_SECS, SWING_SECS, LEVER_COOLDOWN_SECS, LEVER_PRICE), (2.0, 5.0, 30.0, 500));
    }

    #[test]
    fn in_way_covers_the_deck_the_sweep_and_the_water_under_them() {
        let piece = market();
        let at = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
        // On the deck at Span, either way round.
        assert!(piece.in_way(at(-22.0, 0.0, -6.0), 0, 1));
        assert!(piece.in_way(at(-22.0, 0.0, -6.0), 1, 0));
        // In the sweep: over the water the deck passes but is at neither Rest.
        assert!(piece.in_way(at(-17.0, 0.0, -2.0), 0, 1));
        // On the Along deck, over the water by the bank.
        assert!(piece.in_way(at(-12.0, 0.0, -10.0), 0, 1));
        // In the water under the deck, and under the sweep.
        assert!(piece.in_way(at(-22.0, -4.0, -6.0), 0, 1));
        assert!(piece.in_way(at(-17.0, -3.0, -2.0), 0, 1));
        // Beside it, on the quay, or beyond the sweep's reach.
        assert!(!piece.in_way(at(-27.0, 0.0, -6.0), 0, 1));
        assert!(!piece.in_way(at(-22.0, 0.0, -16.0), 0, 1));
        assert!(!piece.in_way(at(-22.0, 0.0, 3.0), 0, 1));
        // Out of the water's reach below, or well above a deck.
        assert!(!piece.in_way(at(-22.0, -9.0, -6.0), 0, 1));
        assert!(!piece.in_way(at(-22.0, 6.0, -6.0), 0, 1));
        // A Piece standing still (the same Rest) covers just its deck.
        assert!(piece.in_way(at(-22.0, 0.0, -6.0), 0, 0));
        assert!(!piece.in_way(at(-17.0, 0.0, -2.0), 0, 0));
    }

    #[test]
    fn the_cycle_is_warning_waiting_swinging_cooling_then_rest() {
        let mut phase = PiecePhase::Rest(0);
        phase.pull(1);
        assert_eq!(phase, PiecePhase::Warning { from: 0, to: 1, elapsed: 0.0 });
        assert_eq!(phase.heading(), Some((0, 1)));
        assert!(phase.bell() && phase.lamps() && !phase.idle());
        phase.advance(SWING_WARN_SECS - 0.1, true, LEVER_COOLDOWN_SECS);
        assert!(matches!(phase, PiecePhase::Warning { .. }));
        phase.advance(0.1, false, LEVER_COOLDOWN_SECS);
        assert_eq!(phase, PiecePhase::Waiting { from: 0, to: 1 });
        // Waiting has no time limit.
        assert_eq!(phase.advance(1000.0, false, LEVER_COOLDOWN_SECS), Beat::default());
        assert_eq!(phase, PiecePhase::Waiting { from: 0, to: 1 });
        let beat = phase.advance(0.0, true, LEVER_COOLDOWN_SECS);
        assert_eq!(beat.started, Some((0, 1)));
        assert_eq!(phase, PiecePhase::Swinging { from: 0, to: 1, elapsed: 0.0 });
        assert_eq!(phase.standing(), None);
        let beat = phase.advance(SWING_SECS, true, LEVER_COOLDOWN_SECS);
        assert_eq!(beat.landed, Some(1));
        assert_eq!(phase, PiecePhase::Cooling { at: 1, elapsed: 0.0 });
        assert_eq!(phase.standing(), Some(1));
        phase.advance(LEVER_COOLDOWN_SECS - 0.1, true, LEVER_COOLDOWN_SECS);
        assert!(matches!(phase, PiecePhase::Cooling { .. }));
        phase.advance(0.1, true, LEVER_COOLDOWN_SECS);
        assert_eq!(phase, PiecePhase::Rest(1));
        // Only a Piece at Rest can be pulled; the next pull wraps.
        let mut busy = PiecePhase::Cooling { at: 1, elapsed: 3.0 };
        busy.pull(0);
        assert_eq!(busy, PiecePhase::Cooling { at: 1, elapsed: 3.0 });
        assert_eq!(market().next_rest(1), 0);
    }

    #[test]
    fn the_levers_say_why_they_cant_be_pulled() {
        let swing = LeverPrompt::of(PiecePhase::Rest(0), false);
        assert_eq!((swing, swing.text()), (LeverPrompt::Swing, "Press F to swing the bridge"));
        assert_eq!(LeverPrompt::of(PiecePhase::Waiting { from: 0, to: 1 }, false).text(), "Clear the bridge");
        assert_eq!(LeverPrompt::of(PiecePhase::Warning { from: 0, to: 1, elapsed: 0.0 }, false).text(), "Clear the bridge");
        assert_eq!(LeverPrompt::of(PiecePhase::Swinging { from: 0, to: 1, elapsed: 1.0 }, false).text(), "The bridge is swinging");
        assert_eq!(LeverPrompt::of(PiecePhase::Cooling { at: 1, elapsed: 1.0 }, false).text(), "The bridge is settling");
        assert_eq!(LeverPrompt::of(PiecePhase::Rest(0), true), LeverPrompt::Dead);
    }

    #[test]
    fn only_a_piece_standing_at_a_rest_with_its_gate_open_joins_zones() {
        let piece = market();
        assert_eq!(piece.open_joins(Some(0), true), [(TownSquare, LockYard)]);
        assert!(piece.open_joins(Some(0), false).is_empty(), "the gate Door is shut");
        assert!(piece.open_joins(Some(1), true).is_empty(), "Along joins nothing");
        assert!(piece.open_joins(None, true).is_empty(), "none while it moves");
        assert_eq!(PiecePhase::Warning { from: 0, to: 1, elapsed: 0.5 }.standing(), Some(0));
        assert_eq!(PiecePhase::Swinging { from: 0, to: 1, elapsed: 0.5 }.standing(), None);
    }

    // ---- Per Map ----

    #[test]
    fn only_the_polder_the_sanatorium_and_the_archive_have_pieces_the_polders_four_bridges_each_with_span_and_along_and_two_lock_gates() {
        for map in MapId::ALL {
            let want = match map {
                MapId::Polder | MapId::Sanatorium => 6,
                MapId::Archive => 5,
                _ => 0,
            };
            assert_eq!(map.layout().shifting.len(), want, "{}", map.name());
        }
        for (id, piece) in pieces(polder()).take(4) {
            assert!(matches!(piece.driver, Driver::Levers(_)));
            assert_eq!(piece.leaves.len(), 1, "{id:?}");
            assert_eq!((piece.rests.len(), piece.start), (2, 0));
            assert_eq!(piece.rests[0].joins.len(), 1);
            assert!(piece.rests[1].joins.is_empty(), "Along joins nothing");
            assert_eq!(piece.levers().len(), 2, "a Lever at each landing");
            assert!(piece.door.is_some());
        }
    }

    /// No path node ever sits on a Leaf, at any Rest.
    #[test]
    fn no_path_node_sits_on_a_leaf_at_any_rest() {
        let graph = NavGraph::new(MapId::Polder);
        for (id, piece) in pieces(polder()) {
            for node in &graph.nodes {
                for rest in piece.rests {
                    for (leaf, &yaw) in piece.leaves.iter().zip(rest.at) {
                        assert!(!leaf.over(node.at, yaw, 0.0), "{} is on a Leaf of {id:?}", node.name);
                    }
                }
            }
        }
    }

    #[test]
    fn every_rest_edge_joins_the_two_zones_its_rest_joins() {
        let graph = NavGraph::new(MapId::Polder);
        let mut seen = 0;
        for edge in &graph.edges {
            let Gate::Rest(id, rest) = edge.gate else { continue };
            seen += 1;
            let joins = polder().shifting[usize::from(id.0)].rests[usize::from(rest)].joins;
            let zones = (graph.nodes[edge.from].zone.unwrap(), graph.nodes[edge.to].zone.unwrap());
            assert!(joins.iter().any(|&(a, b)| (a, b) == zones || (b, a) == zones), "{} -> {}: {zones:?} vs {joins:?}", graph.nodes[edge.from].name, graph.nodes[edge.to].name);
        }
        assert_eq!(seen, 6, "four bridges and two gates, each with one Rest edge");
    }

    /// Every Lever is on its landing's Zone, clear of every sweep, and outside its pen: not
    /// within a deck's width of the deck line.
    #[test]
    fn every_lever_is_clear_of_every_sweep_and_outside_its_pen() {
        for (id, piece) in pieces(polder()) {
            for &(zone, at, facing) in piece.levers() {
                assert_eq!(crate::enemies::nav::zone_at(MapId::Polder, at + Vec3::Y * 0.1), Some(zone), "{id:?}");
                assert!((facing.length() - 1.0).abs() < 1e-5);
                for (other, other_piece) in pieces(polder()) {
                    for rest in 0..other_piece.rests.len() as u8 {
                        let to = other_piece.next_rest(rest);
                        assert!(!other_piece.in_way(at, rest, to), "a Lever of {id:?} is in the way of {other:?}");
                    }
                }
                let leaf = piece.leaves[0];
                let span = piece.rests[0].at[0];
                let off_the_line = (leaf.centre(span).x - at.x).abs();
                assert!(off_the_line > leaf.size.x / 2.0 + 0.5, "a Lever of {id:?} is inside its pen's width");
            }
        }
    }

    /// With every Door bought, every node reaches the Town Square's in all 16 combinations of the
    /// four bridges' Rests.
    #[test]
    fn every_zone_reaches_the_town_square_in_all_sixteen_bridge_rest_combinations() {
        let graph = NavGraph::new(MapId::Polder);
        let doors = all_doors();
        let yes = |_: crate::maps::map::BarrierId| true;
        let start = graph.nearest(polder().seat_start(0));
        for combo in 0..16u8 {
            // The gates stand at Low's Rests: the Inland Gate Shut and the Sea Gate Open.
            let mut rests: Vec<Option<u8>> = (0..4).map(|i| Some((combo >> i) & 1)).collect();
            rests.extend([Some(0), Some(1)]);
            let state = NavState::new(&doors, &yes).with_rests(&rests);
            for (to, node) in graph.nodes.iter().enumerate() {
                if node.zone.is_none() || node.vent || node.name.ends_with("_out") {
                    continue;
                }
                assert!(graph.plan(to, start, state).is_some(), "{} is cut off from the Square with Rests {combo:04b}", node.name);
            }
        }
    }

    #[test]
    fn a_route_crosses_a_bridge_only_at_span() {
        let graph = NavGraph::new(MapId::Polder);
        let doors = all_doors();
        let yes = |_: crate::maps::map::BarrierId| true;
        let (near, far) = (graph.node("mb_pi").unwrap(), graph.node("mb_far").unwrap());
        let crosses = |rests: &[Option<u8>]| {
            let path = graph.plan(near, far, NavState::new(&doors, &yes).with_rests(rests)).unwrap();
            path.windows(2).any(|pair| pair == [near, far])
        };
        assert!(crosses(&[Some(0), Some(0), Some(0), Some(0)]), "at Span it walks the deck");
        assert!(!crosses(&[Some(1), Some(0), Some(0), Some(0)]), "at Along it goes round");
        assert!(!crosses(&[None, Some(0), Some(0), Some(0)]), "while it moves it goes round");
        // With no Door bought there is no way round at all.
        let state = NavState::new(&[], &yes).with_rests(&[Some(1), Some(0), Some(0), Some(0)]);
        assert!(graph.plan(near, far, state).is_none());
    }

    // ---- Headless ----

    /// The Polder with the Shifting systems.
    fn polder_sim() -> Sim {
        Sim::on(MapId::Polder, 60.0, |app| {
            app.add_plugins(ShiftingPlugin)
                .insert_resource(ActiveZones(Default::default()))
                .init_resource::<Kills>()
                .add_message::<Purchased>()
                .add_message::<PlayCue>()
                .add_message::<Teleport>()
                .init_resource::<Tide>()
                .init_resource::<Swings>()
                .add_systems(Update, (log_kills, log_swings));
        })
    }

    #[derive(Resource, Default)]
    struct Kills(Vec<(Attack, Pay)>);

    /// Every Player a `Teleport` was sent for.
    #[derive(Resource, Default)]
    struct Teleported(Vec<Entity>);

    fn log_teleports(mut teleported: ResMut<Teleported>, mut heard: MessageReader<Teleport>) {
        teleported.0.extend(heard.read().map(|teleport| teleport.player));
    }

    fn log_kills(mut kills: ResMut<Kills>, mut heard: MessageReader<ZombieKilled>) {
        kills.0.extend(heard.read().map(|kill| (kill.attack, kill.pay)));
    }

    fn phase(sim: &mut Sim, id: u8) -> PiecePhase {
        let world = sim.world_mut();
        world.query::<&PieceState>().iter(world).find(|state| state.id() == PieceId(id)).unwrap().phase()
    }

    fn show(sim: &mut Sim, id: u8, phase: PiecePhase) {
        let world = sim.world_mut();
        world.query::<&mut PieceState>().iter_mut(world).find(|state| state.id() == PieceId(id)).unwrap().show(phase);
    }

    fn lever(sim: &mut Sim, id: u8, nth: usize) -> Entity {
        let world = sim.world_mut();
        world.query::<(Entity, &Lever)>().iter(world).filter(|(_, lever)| lever.piece == PieceId(id)).nth(nth).unwrap().0
    }

    fn move_player(sim: &mut Sim, feet: Vec3) {
        let world = sim.world_mut();
        let half = crate::players::player::Stance::Standing.half_height();
        let mut query = world.query_filtered::<&mut Transform, With<Player>>();
        query.iter_mut(world).next().unwrap().translation = feet + Vec3::Y * half;
    }

    fn pull(sim: &mut Sim, lever: Entity) {
        let world = sim.world_mut();
        let player = world.query_filtered::<Entity, With<Player>>().iter(world).next().unwrap();
        world.write_message(Purchased { player, buyable: lever });
    }

    fn leaf_rotation(sim: &mut Sim, id: u8) -> Quat {
        let world = sim.world_mut();
        let mut query = world.query::<(&LeafBody, &Transform)>();
        query.iter(world).find(|(body, _)| body.piece == PieceId(id)).unwrap().1.rotation
    }

    /// A Lever waits while a Player is on the deck and Swings once they step off.
    #[test]
    fn a_lever_waits_while_a_player_is_on_the_deck_and_swings_once_they_step_off() {
        let mut sim = polder_sim();
        move_player(&mut sim, Vec3::new(-22.0, 0.0, -6.0));
        let pulled = lever(&mut sim, 0, 0);
        sim.run(30);
        pull(&mut sim, pulled);
        sim.run(sim.frames(0.5));
        assert!(matches!(phase(&mut sim, 0), PiecePhase::Warning { .. }));
        sim.run(sim.frames(SWING_WARN_SECS + 3.0));
        assert_eq!(phase(&mut sim, 0), PiecePhase::Waiting { from: 0, to: 1 }, "no time limit while they stand there");
        move_player(&mut sim, Vec3::new(-30.0, 0.0, -20.0));
        sim.run(sim.frames(0.5));
        assert!(phase(&mut sim, 0).swinging(), "{:?}", phase(&mut sim, 0));
        sim.run(sim.frames(SWING_SECS + 0.5));
        assert!(matches!(phase(&mut sim, 0), PiecePhase::Cooling { at: 1, .. }));
        assert_eq!(leaf_rotation(&mut sim, 0), Quat::from_rotation_y(FRAC_PI_2), "it lands exactly on Along");
        assert_eq!(phase(&mut sim, 1), PiecePhase::Rest(0), "no other bridge moved");
    }

    /// Both Levers of a bridge show why they can't be pulled, until the cooldown ends.
    #[test]
    fn the_cooldown_is_shared_between_a_bridges_levers() {
        let mut sim = polder_sim();
        move_player(&mut sim, Vec3::new(-30.0, 0.0, -20.0));
        let (a, b) = (lever(&mut sim, 0, 0), lever(&mut sim, 0, 1));
        sim.run(5);
        for l in [a, b] {
            let buyable = sim.world_mut().get::<Buyable>(l).unwrap();
            assert_eq!((buyable.prompt.as_str(), buyable.price), ("Press F to swing the bridge", Some(LEVER_PRICE)));
        }
        pull(&mut sim, b);
        sim.run(sim.frames(SWING_WARN_SECS + SWING_SECS + 1.0));
        for l in [a, b] {
            let world = sim.world_mut();
            let buyable = world.get::<Buyable>(l).unwrap();
            assert_eq!((buyable.prompt.as_str(), buyable.price), ("The bridge is settling", None));
            assert!(world.get::<Refused>(l).is_some());
        }
        // Another bridge's Levers are untouched.
        let other = lever(&mut sim, 1, 0);
        assert_eq!(sim.world_mut().get::<Buyable>(other).unwrap().price, Some(LEVER_PRICE));
        sim.run(sim.frames(LEVER_COOLDOWN_SECS));
        for l in [a, b] {
            assert_eq!(sim.world_mut().get::<Buyable>(l).unwrap().price, Some(LEVER_PRICE), "ready again");
        }
        assert_eq!(phase(&mut sim, 0), PiecePhase::Rest(1));
    }

    /// A Zombie on the deck is Tipped as an Unpaid Kill that counts; one on the quay is left alone.
    #[test]
    fn a_zombie_on_the_deck_is_tipped_as_an_unpaid_kill_and_one_on_the_quay_is_not() {
        let mut sim = polder_sim();
        move_player(&mut sim, Vec3::new(-30.0, 0.0, -30.0));
        let on_deck = sim.spawn(Vec3::new(-22.0, 0.0, -6.0), None);
        let on_quay = sim.spawn(Vec3::new(-22.0, 0.0, -20.0), None);
        sim.send(SwingStarted { piece: PieceId(0), from: 0, to: 1 });
        sim.run(4);
        assert!(sim.world_mut().get::<Dying>(on_deck).is_some(), "tipped");
        assert!(sim.world_mut().get::<Tipped>(on_deck).is_some());
        assert!(sim.world_mut().get::<Dying>(on_quay).is_none());
        assert_eq!(sim.world_mut().resource::<Kills>().0, [(Attack::Swing, Pay::Unpaid)], "it counts, and pays nothing");
        // It is thrown into the canal and is gone.
        sim.run(sim.frames(TIPPED_FALL_SECS + 0.5));
        assert!(sim.world_mut().get_entity(on_deck).is_err());
    }

    /// A Sapper is not killed: it loses the Trap's share of its health and is moved.
    #[test]
    fn a_tipped_sapper_loses_its_share_and_is_moved() {
        let mut sim = polder_sim();
        move_player(&mut sim, Vec3::new(-30.0, 0.0, -30.0));
        let on_deck = Vec3::new(-22.0, 0.0, -6.0);
        let sapper = sim.spawn(on_deck, None);
        sim.world_mut().entity_mut(sapper).insert((Zombie::special(SpecialKind::Sapper, 1000.0, on_deck), Heavyweight::SAPPER));
        sim.world_mut().resource_mut::<ActiveZones>().0 = crate::maps::zone::ZoneSet::from_iter(ZoneId::ALL);
        sim.send(SwingStarted { piece: PieceId(0), from: 0, to: 1 });
        sim.run(4);
        let world = sim.world_mut();
        assert!(world.get::<Dying>(sapper).is_none(), "it lives");
        let health = world.get::<Zombie>(sapper).unwrap().health();
        assert!((health - 1000.0 * (1.0 - TRAP_SAPPER_HEALTH_FRACTION)).abs() < 1.0, "{health}");
        let at = world.get::<Transform>(sapper).unwrap().translation;
        assert!(!polder().shifting[0].in_way(at - Vec3::Y * 1.0, 0, 1), "moved off the bridge to {at}");
    }

    /// A closed gate Door keeps the far Zone inactive, and so does a bridge at Along or moving.
    #[test]
    fn a_closed_gate_door_keeps_the_far_zone_inactive() {
        use crate::maps::door::Door;
        let mut sim = polder_sim();
        let active = |sim: &mut Sim| {
            sim.world_mut().run_system_once(update_active_zones).unwrap();
            sim.world_mut().resource::<ActiveZones>().0
        };
        let world = sim.world_mut();
        let mut zones = world.query::<&mut PlayerZone>();
        zones.iter_mut(world).next().unwrap().0 = TownSquare;
        assert!(!active(&mut sim).contains(LockYard), "gate Door shut");
        let mut doors = sim.world_mut().query::<&mut Door>();
        for mut door in doors.iter_mut(sim.world_mut()) {
            if door.gate == Some((TownSquare, LockYard)) {
                assert_eq!(door.joins, None, "a gate Door joins nothing");
                door.open = true;
            }
        }
        assert!(active(&mut sim).contains(LockYard), "open, with the bridge at Span");
        show(&mut sim, 0, PiecePhase::Rest(1));
        assert!(!active(&mut sim).contains(LockYard), "Along joins nothing");
        show(&mut sim, 0, PiecePhase::Swinging { from: 1, to: 0, elapsed: 1.0 });
        assert!(!active(&mut sim).contains(LockYard), "none count while it moves");
        show(&mut sim, 0, PiecePhase::Cooling { at: 0, elapsed: 1.0 });
        assert!(active(&mut sim).contains(LockYard), "standing at Span again");
    }

    /// With each Piece at each Rest, every `Gate::Rest` edge for that Rest is a clear walk.
    #[test]
    fn with_each_piece_at_each_rest_every_rest_edge_is_a_clear_walk() {
        use crate::layers::GameLayer;
        let mut sim = polder_sim();
        let graph = NavGraph::new(MapId::Polder);
        let mut walked = 0;
        // the Lock Gates stand wherever this test shows them, whatever the Tide would have them do
        sim.world_mut().remove_resource::<Tide>();
        for combo in 0..64u8 {
            for i in 0..6u8 {
                show(&mut sim, i, PiecePhase::Rest((combo >> i) & 1));
            }
            sim.run(30);
            let blocked: Vec<String> = sim
                .world_mut()
                .run_system_once(move |spatial: SpatialQuery, doors: Query<Entity, With<crate::maps::door::Door>>| -> Vec<String> {
                    let graph = NavGraph::new(MapId::Polder);
                    let filter = SpatialQueryFilter::from_mask(GameLayer::World).with_excluded_entities(doors.iter());
                    let body = Collider::sphere(ZOMBIE_RADIUS * 0.8);
                    graph
                        .edges
                        .iter()
                        .filter(|edge| matches!(edge.gate, Gate::Rest(id, rest) if (combo >> id.0) & 1 == rest))
                        .filter_map(|edge| {
                            let (from, to) = (graph.nodes[edge.from].at + Vec3::Y, graph.nodes[edge.to].at + Vec3::Y);
                            let dir = Dir3::new(to - from).ok()?;
                            let config = ShapeCastConfig::from_max_distance(from.distance(to));
                            let name = format!("{} -> {} at {combo:04b}", graph.nodes[edge.from].name, graph.nodes[edge.to].name);
                            // Walked every half metre, it has a floor under it: the deck, not water.
                            let floorless = (0..=(from.distance(to) * 2.0) as u32).any(|step| {
                                let at = from + (to - from) * (step as f32 / (from.distance(to) * 2.0).max(1.0)).min(1.0);
                                spatial.cast_ray(at, Dir3::NEG_Y, 1.4, true, &filter).is_none()
                            });
                            if floorless {
                                return Some(format!("{name}: no floor"));
                            }
                            spatial.cast_shape(&body, from, Quat::IDENTITY, dir, &config, &filter).map(|_| name)
                        })
                        .collect()
                })
                .unwrap();
            assert!(blocked.is_empty(), "{blocked:?}");
            walked += graph.edges.iter().filter(|edge| matches!(edge.gate, Gate::Rest(id, rest) if (combo >> id.0) & 1 == rest)).count();
        }
        // Each of six edges is open in half the combinations.
        assert_eq!(walked, 6 * 32);
    }

    // ---- Moving Stacks ----

    fn archive() -> &'static Layout {
        MapId::Archive.layout()
    }

    /// The Great Range: four Bookcases on a rail along x from x -3.9, five Rests.
    fn great_range() -> &'static Piece {
        &archive().shifting[0]
    }

    #[test]
    fn rail_pose_lands_exactly_at_each_rest() {
        for (id, piece) in pieces(archive()) {
            assert!(piece.leaves.iter().all(|leaf| matches!(leaf.motion, Motion::Rail(_))), "{id:?}");
            let rests = piece.rests.len() as u8;
            for from in 0..rests {
                for to in 0..rests {
                    assert_eq!(piece.poses(from, to, 0.0), piece.rests[usize::from(from)].at, "{id:?} {from} to {to} starts at its Rest");
                    assert_eq!(piece.poses(from, to, SWING_SECS), piece.rests[usize::from(to)].at, "{id:?} {from} to {to} lands exactly");
                    assert_eq!(piece.poses(from, to, SWING_SECS + 3.0), piece.rests[usize::from(to)].at);
                    for (i, half) in piece.poses(from, to, SWING_SECS / 2.0).into_iter().enumerate() {
                        let (a, b) = (piece.rests[usize::from(from)].at[i], piece.rests[usize::from(to)].at[i]);
                        assert!((half - (a + b) / 2.0).abs() < 1e-5, "{id:?} smoothstep is halfway at the middle");
                    }
                }
            }
            // A phase poses the Leaves where it stands or is.
            let swinging = PiecePhase::Swinging { from: 0, to: rests - 1, elapsed: SWING_SECS };
            assert_eq!(swinging.poses(piece), piece.rests[usize::from(rests - 1)].at);
            assert_eq!(PiecePhase::Rest(piece.start).poses(piece), piece.rests[usize::from(piece.start)].at);
        }
        // The Great Range at its middle Rest: Bookcases 0 and 1 at the west end, 2 and 3 at the east, the Aisle between.
        let range = great_range();
        let centres: Vec<f32> = range.leaves.iter().zip(range.rests[2].at).map(|(leaf, &pose)| leaf.centre(pose).x).collect();
        for (got, want) in centres.iter().zip([-3.3, -2.1, 2.1, 3.3]) {
            assert!((got - want).abs() < 1e-4, "{centres:?}");
        }
        for (leaf, &pose) in range.leaves.iter().zip(range.rests[2].at) {
            assert_eq!(leaf.rotation(pose), Quat::IDENTITY, "a Bookcase never turns");
            assert_eq!(leaf.centre(pose).y, 8.0 + 1.5, "stands on the Reading level, 3 m tall");
            assert_eq!(leaf.centre(pose).z, 12.0);
        }
        // The other way round: RB's rail runs along z.
        let rb = &archive().shifting[2];
        assert_eq!(rb.leaves[0].motion, Motion::Rail(Vec2::Y));
        let z: Vec<f32> = rb.leaves.iter().zip(rb.rests[0].at).map(|(leaf, &pose)| leaf.centre(pose).z).collect();
        for (got, want) in z.iter().zip([26.8 + 0.6 + 3.0, 26.8 + 1.8 + 3.0]) {
            assert!((got - want).abs() < 1e-4, "{z:?}");
        }
    }

    #[test]
    fn rail_in_way_covers_the_closing_aisle_and_not_the_open_one() {
        let range = great_range();
        let at = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
        // Rolling from Rest 2 (the Aisle in the middle, x -1.5 to 1.5) to Rest 0 (the Aisle at the west end): the middle Aisle is
        // the one that closes, and a Player standing in it is in the way.
        assert!(range.in_way(at(0.0, 8.0, 12.0), 2, 0));
        assert!(range.in_way(at(1.0, 8.0, 11.0), 2, 0));
        assert!(range.in_way(at(-1.4, 8.0, 14.0), 2, 0), "anywhere along the Bookcase's length");
        // Standing still at Rest 2, nothing is in the way in the open Aisle: it is clear.
        assert!(!range.in_way(at(0.0, 8.0, 12.0), 2, 2));
        assert!(!range.in_way(at(1.0, 8.0, 11.0), 2, 2));
        // The Bookcases themselves are in the way where they stand.
        assert!(range.in_way(at(-3.3, 8.0, 12.0), 2, 2));
        assert!(range.in_way(at(3.3, 8.0, 12.0), 2, 2));
        // Out past the mouth strips, beyond the rail's ends and a storey up or down are in no one's way.
        assert!(!range.in_way(at(0.0, 8.0, 12.0 + 3.1), 2, 0));
        assert!(!range.in_way(at(0.0, 8.0, 12.0 - 3.1), 2, 0));
        assert!(!range.in_way(at(5.0, 8.0, 12.0), 2, 0));
        assert!(!range.in_way(at(-5.0, 8.0, 12.0), 2, 0));
        assert!(!range.in_way(at(0.0, 8.0 + 3.5, 12.0), 2, 0));
        assert!(!range.in_way(at(0.0, 4.0, 12.0), 2, 0), "the Stacks level below");
        // Rolling from Rest 0 to Rest 1 moves no Bookcase but the first: the Aisle that closes (x -3.9 to -0.9) is in the way,
        // and the far end of the rail is only where the Bookcase there stands.
        assert!(range.in_way(at(-3.0, 8.0, 12.0), 0, 1));
        assert!(!range.in_way(at(3.0, 8.0, 12.0 + 3.1), 0, 1));
    }

    #[test]
    fn a_wheel_opens_its_own_rest() {
        for (id, piece) in pieces(archive()) {
            let wheels = piece.wheels();
            assert_eq!(wheels.len(), 2 * piece.rests.len(), "{id:?}: a Wheel at each mouth of every Aisle");
            for at in 0..piece.rests.len() as u8 {
                for wheel in wheels {
                    if wheel.rest == at {
                        continue;
                    }
                    assert_eq!(wheel.opens(PiecePhase::Rest(at)), Some(wheel.rest), "{id:?}: the Wheel for Rest {} rolls the Range there from {at}", wheel.rest);
                }
            }
            for rest in 0..piece.rests.len() as u8 {
                assert_eq!(wheels.iter().filter(|wheel| wheel.rest == rest).count(), 2, "{id:?}: two Wheels for Rest {rest}");
            }
            assert!(piece.levers().is_empty(), "a Range has no Levers");
        }
    }

    #[test]
    fn a_wheel_at_the_open_aisle_does_nothing() {
        for (id, piece) in pieces(archive()) {
            for wheel in piece.wheels() {
                assert_eq!(wheel.opens(PiecePhase::Rest(wheel.rest)), None, "{id:?}");
                // Busy, a Range turns to no Wheel at all.
                for busy in [
                    PiecePhase::Warning { from: 0, to: 1, elapsed: 0.5 },
                    PiecePhase::Waiting { from: 0, to: 1 },
                    PiecePhase::Swinging { from: 0, to: 1, elapsed: 1.0 },
                    PiecePhase::Cooling { at: 1, elapsed: 1.0 },
                ] {
                    assert_eq!(wheel.opens(busy), None, "{id:?}: {busy:?}");
                }
            }
        }
    }

    #[test]
    fn range_cooldown_is_8_seconds() {
        assert_eq!(RANGE_COOLDOWN_SECS, 8.0);
        assert_eq!(great_range().cooldown(), 8.0);
        assert_eq!(market().cooldown(), LEVER_COOLDOWN_SECS, "a Lever's is still 30 s");
        let mut phase = PiecePhase::Cooling { at: 0, elapsed: 0.0 };
        phase.advance(RANGE_COOLDOWN_SECS - 0.1, true, great_range().cooldown());
        assert!(matches!(phase, PiecePhase::Cooling { .. }));
        phase.advance(0.1, true, great_range().cooldown());
        assert_eq!(phase, PiecePhase::Rest(0));
        // The whole cycle: warning, wait, roll and cooldown.
        let mut cycle = PiecePhase::Rest(2);
        cycle.pull(0);
        let mut left = SWING_WARN_SECS + SWING_SECS + RANGE_COOLDOWN_SECS;
        while left > 0.0 {
            cycle.advance(0.5_f32.min(left), true, great_range().cooldown());
            left -= 0.5;
        }
        assert_eq!(cycle, PiecePhase::Rest(0));
    }

    #[test]
    fn wheel_prompt_text_by_phase() {
        let text = |phase, rest| WheelPrompt::of(phase, rest, false).text();
        // At Rest 2 the Wheel for Rest 0 can open it; the Wheel for Rest 2 is at the open Aisle.
        assert_eq!(text(PiecePhase::Rest(2), 0), "Press F to open this aisle");
        assert_eq!(text(PiecePhase::Rest(2), 2), "");
        assert_eq!(text(PiecePhase::Warning { from: 2, to: 0, elapsed: 0.5 }, 0), "Clear the aisle");
        assert_eq!(text(PiecePhase::Waiting { from: 2, to: 0 }, 0), "Clear the aisle");
        assert_eq!(text(PiecePhase::Waiting { from: 2, to: 0 }, 4), "Clear the aisle");
        assert_eq!(text(PiecePhase::Swinging { from: 2, to: 0, elapsed: 1.0 }, 0), "The stacks are rolling");
        assert_eq!(text(PiecePhase::Swinging { from: 2, to: 0, elapsed: 1.0 }, 2), "The stacks are rolling");
        assert_eq!(text(PiecePhase::Cooling { at: 0, elapsed: 1.0 }, 3), "The stacks are settling");
        assert_eq!(text(PiecePhase::Cooling { at: 0, elapsed: 1.0 }, 0), "", "nothing at the Aisle that is open");
        assert_eq!(WheelPrompt::of(PiecePhase::Rest(2), 0, true), WheelPrompt::None, "Sabotaged: dead");
        assert_eq!(WheelPrompt::of(PiecePhase::Rest(2), 0, false), WheelPrompt::Open);
    }

    /// The Polder's and the Sanatorium's Leaves turn exactly as they did: the yaw they pose at is the Rest's, and the frame, centre and
    /// footprint come out of the same arithmetic as before the Rail came to be.
    #[test]
    fn hinge_pieces_pose_as_before() {
        let mut leaves = 0;
        for map in [MapId::Polder, MapId::Sanatorium] {
            for (id, piece) in pieces(map.layout()) {
                for (rest, standing) in piece.rests.iter().enumerate() {
                    assert_eq!(standing.at.len(), piece.leaves.len(), "{id:?}");
                    for (leaf, &yaw) in piece.leaves.iter().zip(standing.at) {
                        leaves += 1;
                        assert_eq!(leaf.motion, Motion::Hinge, "{id:?} rest {rest}");
                        assert_eq!(leaf.origin(yaw), leaf.hinge);
                        assert_eq!(leaf.rotation(yaw), Quat::from_rotation_y(yaw));
                        assert_eq!(leaf.centre(yaw), leaf.hinge + Quat::from_rotation_y(yaw) * leaf.offset);
                        // Over the box's own centre, and not a world away.
                        assert!(leaf.over(leaf.centre(yaw), yaw, 0.0));
                        assert!(!leaf.over(leaf.centre(yaw) + Vec3::new(50.0, 0.0, 0.0), yaw, 0.0));
                        // The old footprint test, written out.
                        for probe in [Vec3::new(0.3, 0.0, 0.2), Vec3::new(-1.1, 0.0, 2.2), Vec3::new(4.0, 0.0, -3.0)] {
                            let at = leaf.centre(yaw) + probe;
                            let local = Quat::from_rotation_y(-yaw) * (at - leaf.hinge) - leaf.offset;
                            let before = local.x.abs() <= leaf.size.x / 2.0 + 0.3 && local.z.abs() <= leaf.size.z / 2.0 + 0.3;
                            assert_eq!(leaf.over(at, yaw, 0.3), before);
                        }
                    }
                }
            }
        }
        assert!(leaves > 30, "{leaves}");
        // A Swing Bridge still turns a quarter turn, over the same five seconds, and a Lever's cooldown is still 30 s.
        let bridge = market();
        assert_eq!(bridge.poses(0, 1, SWING_SECS), [FRAC_PI_2]);
        assert!((bridge.poses(0, 1, SWING_SECS / 2.0)[0] - FRAC_PI_2 / 2.0).abs() < 1e-5);
        assert_eq!(bridge.cooldown(), LEVER_COOLDOWN_SECS);
        assert_eq!(MapId::Sanatorium.layout().shifting[0].cooldown(), LEVER_COOLDOWN_SECS);
    }

    // ---- Headless: the Archive's Ranges ----

    /// The Archive with the Shifting systems, every Zone Active.
    fn archive_sim() -> Sim {
        let mut sim = Sim::on(MapId::Archive, 60.0, |app| {
            app.add_plugins(ShiftingPlugin)
                .insert_resource(ActiveZones(Default::default()))
                .init_resource::<Kills>()
                .add_message::<Purchased>()
                .add_message::<PlayCue>()
                .add_message::<Teleport>()
                .init_resource::<Swings>()
                .init_resource::<Teleported>()
                .add_systems(Update, (log_kills, log_swings, log_teleports));
        });
        sim.world_mut().resource_mut::<ActiveZones>().0 = crate::maps::zone::ZoneSet::from_iter(ZoneId::ALL);
        sim
    }

    /// Every Door of the Map open, as when every one is bought.
    fn open_every_door(sim: &mut Sim) {
        let world = sim.world_mut();
        let mut doors = world.query::<&mut Door>();
        for mut door in doors.iter_mut(world) {
            door.open = true;
        }
    }

    /// The `nth` (0 the low side, 1 the high) Wheel of Piece `id` that rolls it to `rest`.
    fn wheel(sim: &mut Sim, id: u8, rest: u8, nth: usize) -> Entity {
        let world = sim.world_mut();
        world.query::<(Entity, &WheelPost)>().iter(world).filter(|(_, post)| post.piece == PieceId(id) && post.rest == rest).nth(nth).unwrap().0
    }

    /// Uses the `nth` Wheel of Piece `id` for `rest`, as the first Player.
    fn pull_wheel(sim: &mut Sim, id: u8, rest: u8, nth: usize) {
        let wheel = wheel(sim, id, rest, nth);
        pull(sim, wheel);
    }

    /// What the `nth` Wheel of Piece `id` for `rest` says.
    fn says(sim: &mut Sim, id: u8, rest: u8, nth: usize) -> Option<(String, Option<u64>, bool)> {
        let wheel = wheel(sim, id, rest, nth);
        wheel_says(sim, wheel)
    }

    /// What a Wheel's Buyable says now: its prompt and whether it is refused, or `None` with no Buyable.
    fn wheel_says(sim: &mut Sim, wheel: Entity) -> Option<(String, Option<u64>, bool)> {
        let world = sim.world_mut();
        world.get::<Buyable>(wheel).map(|buyable| (buyable.prompt.clone(), buyable.price, world.get::<Refused>(wheel).is_some()))
    }

    /// The Range rests at its start, every Wheel shows what it should and a use rolls it through warning, roll and cooldown, to its own Rest.
    #[test]
    fn a_wheel_use_rolls_the_range_through_its_whole_cycle_and_the_wheels_follow() {
        let mut sim = archive_sim();
        move_player(&mut sim, Vec3::new(-30.0, 8.0, 30.0));
        sim.run(30);
        let (to_rest_0, at_open_aisle, other_range) = (wheel(&mut sim, 0, 0, 0), wheel(&mut sim, 0, 2, 0), wheel(&mut sim, 1, 1, 0));
        assert_eq!(phase(&mut sim, 0), PiecePhase::Rest(2), "the Great Range starts with its Aisle in the middle");
        assert_eq!(wheel_says(&mut sim, to_rest_0), Some(("Press F to open this aisle".into(), None, false)), "free, and usable");
        assert_eq!(wheel_says(&mut sim, at_open_aisle), None, "nothing at the open Aisle's Wheels");
        pull(&mut sim, to_rest_0);
        sim.run(sim.frames(0.5));
        assert!(matches!(phase(&mut sim, 0), PiecePhase::Warning { from: 2, to: 0, .. }), "{:?}", phase(&mut sim, 0));
        assert_eq!(wheel_says(&mut sim, to_rest_0), Some(("Clear the aisle".into(), None, true)));
        assert_eq!(wheel_says(&mut sim, at_open_aisle), None);
        sim.run(sim.frames(SWING_WARN_SECS + 0.5));
        assert!(phase(&mut sim, 0).swinging(), "{:?}", phase(&mut sim, 0));
        assert_eq!(wheel_says(&mut sim, at_open_aisle), Some(("The stacks are rolling".into(), None, true)));
        assert_eq!(sim.world_mut().resource::<Swings>().0, [(PieceId(0), 2, 0)]);
        sim.run(sim.frames(SWING_SECS));
        assert!(matches!(phase(&mut sim, 0), PiecePhase::Cooling { at: 0, .. }), "{:?}", phase(&mut sim, 0));
        assert_eq!(says(&mut sim, 0, 3, 1), Some(("The stacks are settling".into(), None, true)));
        assert_eq!(wheel_says(&mut sim, to_rest_0), None, "the Aisle that opened has no Wheel use now");
        // The Bookcases stand where Rest 0 has them: all four against the east end.
        let xs: Vec<f32> = {
            let world = sim.world_mut();
            let mut query = world.query::<(&LeafBody, &Transform)>();
            let mut found: Vec<(usize, f32)> = query.iter(world).filter(|(body, _)| body.piece == PieceId(0)).map(|(body, transform)| (body.index, transform.translation.x)).collect();
            found.sort_by_key(|&(index, _)| index);
            found.into_iter().map(|(_, x)| x).collect()
        };
        let rest = &MapId::Archive.layout().shifting[0];
        for (i, x) in xs.iter().enumerate() {
            assert!((x - (rest.leaves[i].hinge.x + rest.rests[0].at[i])).abs() < 1e-4, "Bookcase {i} at x {x}");
        }
        // A Wheel does nothing while the Range cools, and it comes to Rest after 8 s: the cooldown is the Range's alone.
        pull_wheel(&mut sim, 0, 4, 0);
        sim.run(5);
        assert!(matches!(phase(&mut sim, 0), PiecePhase::Cooling { at: 0, .. }), "a Wheel turns nothing while it settles");
        assert_eq!(phase(&mut sim, 1), PiecePhase::Rest(0), "no other Range moved");
        assert_eq!(wheel_says(&mut sim, other_range), Some(("Press F to open this aisle".into(), None, false)), "another Range's Wheels are untouched");
        sim.run(sim.frames(RANGE_COOLDOWN_SECS));
        assert_eq!(phase(&mut sim, 0), PiecePhase::Rest(0));
        assert_eq!(says(&mut sim, 0, 4, 0), Some(("Press F to open this aisle".into(), None, false)));
        assert_eq!(wheel_says(&mut sim, to_rest_0), None);
    }

    /// A Wheel at the open Aisle, or at a Sabotaged Range, rolls nothing.
    #[test]
    fn a_wheel_at_the_open_aisle_or_a_sabotaged_range_rolls_nothing() {
        let mut sim = archive_sim();
        move_player(&mut sim, Vec3::new(-30.0, 8.0, 30.0));
        sim.run(30);
        pull_wheel(&mut sim, 0, 2, 1);
        sim.run(sim.frames(1.0));
        assert_eq!(phase(&mut sim, 0), PiecePhase::Rest(2), "the open Aisle's own Wheel does nothing");
        let state = {
            let world = sim.world_mut();
            world.query::<(Entity, &PieceState)>().iter(world).find(|(_, state)| state.id() == PieceId(0)).unwrap().0
        };
        sim.world_mut().entity_mut(state).insert(Sabotaged);
        sim.run(5);
        assert_eq!(says(&mut sim, 0, 0, 0), None, "a Sabotaged Range's Wheels are dead");
        pull_wheel(&mut sim, 0, 0, 0);
        sim.run(sim.frames(1.0));
        assert_eq!(phase(&mut sim, 0), PiecePhase::Rest(2), "and turn nothing");
        sim.world_mut().entity_mut(state).remove::<Sabotaged>();
        sim.run(5);
        pull_wheel(&mut sim, 0, 0, 0);
        sim.run(sim.frames(0.5));
        assert!(matches!(phase(&mut sim, 0), PiecePhase::Warning { from: 2, to: 0, .. }), "restored, it turns again");
    }

    /// A Player (Down or not) in the Aisle that is closing holds the Range at Waiting, however long they stand there, and it rolls the moment
    /// they step out.
    #[test]
    fn a_player_in_the_closing_aisle_holds_the_range_at_waiting_and_a_down_player_too() {
        for down in [false, true] {
            let mut sim = archive_sim();
            let in_the_aisle = Vec3::new(0.0, 8.0, 12.0);
            move_player(&mut sim, in_the_aisle);
            if down {
                let world = sim.world_mut();
                let player = world.query_filtered::<Entity, With<Player>>().iter(world).next().unwrap();
                world.entity_mut(player).insert(crate::players::last_stand::LastStand::shown(10.0, false));
            }
            sim.run(30);
            // Use the Wheel on the far side, as the Player (up or Down, the test's one body) stands in the middle of the Aisle.
            pull_wheel(&mut sim, 0, 0, 1);
            sim.run(sim.frames(SWING_WARN_SECS + 4.0));
            assert_eq!(phase(&mut sim, 0), PiecePhase::Waiting { from: 2, to: 0 }, "down {down}: no time limit while they stand there");
            assert!(sim.world_mut().resource::<Swings>().0.is_empty());
            // Stepping out (to the near mouth, outside the sweep) lets it roll.
            move_player(&mut sim, Vec3::new(0.0, 8.0, 12.0 + 3.2));
            sim.run(sim.frames(0.5));
            assert!(phase(&mut sim, 0).swinging(), "down {down}: {:?}", phase(&mut sim, 0));
        }
    }

    /// A Range never kills: `tip_occupants` skips it, so a Zombie, a Napalm and a Sapper in the Aisle that closes are not Tipped, not
    /// killed, not moved to a spawn point and not made an Unpaid Kill.
    #[test]
    fn tip_occupants_skips_ranges() {
        let mut sim = archive_sim();
        move_player(&mut sim, Vec3::new(-30.0, 8.0, 30.0));
        let in_the_aisle = Vec3::new(0.0, 8.0, 12.0);
        let zombie = sim.spawn(in_the_aisle, None);
        let napalm = sim.spawn(in_the_aisle, None);
        sim.world_mut().entity_mut(napalm).insert(Zombie::special(SpecialKind::Napalm, 1000.0, in_the_aisle));
        let sapper = sim.spawn(in_the_aisle, None);
        sim.world_mut().entity_mut(sapper).insert((Zombie::special(SpecialKind::Sapper, 1000.0, in_the_aisle), Heavyweight::SAPPER));
        // The sweep's own start, but with the Shove out of the way: only `tip_occupants` is asked.
        sim.send(SwingStarted { piece: PieceId(0), from: 2, to: 0 });
        sim.run(10);
        for body in [zombie, napalm, sapper] {
            assert!(sim.world_mut().get::<Dying>(body).is_none());
            assert!(sim.world_mut().get::<Tipped>(body).is_none());
            assert!(sim.world_mut().get::<Tossed>(body).is_none());
        }
        assert!(sim.world_mut().resource::<Kills>().0.is_empty());
    }

    /// Bodies of every kind in the closing Aisle when a Roll starts, then three frames on: the Sim, the bodies and the mouths of the Aisle that closed.
    fn shoved_by_a_roll(bodies: &[fn(&mut Sim, Vec3) -> Entity]) -> (Sim, Vec<Entity>, Vec<Vec3>) {
        let mut sim = archive_sim();
        move_player(&mut sim, Vec3::new(-30.0, 8.0, 30.0));
        sim.run(3);
        let mouths = mouths(&NavGraph::new(MapId::Archive), PieceId(0), 2);
        let spots = [Vec3::new(0.2, 8.0, 12.6), Vec3::new(-0.3, 8.0, 11.4), Vec3::new(0.4, 8.0, 12.2), Vec3::new(0.0, 8.0, 11.0)];
        let entities: Vec<Entity> = bodies.iter().zip(spots).map(|(make, at)| make(&mut sim, at)).collect();
        sim.send(SwingStarted { piece: PieceId(0), from: 2, to: 0 });
        sim.run(3);
        (sim, entities, mouths)
    }

    fn feet_of_zombie(sim: &mut Sim, entity: Entity) -> Vec3 {
        let world = sim.world_mut();
        let half = world.get::<Zombie>(entity).unwrap().half_height();
        world.get::<Transform>(entity).unwrap().translation - Vec3::Y * half
    }

    fn at_a_mouth(feet: Vec3, mouths: &[Vec3]) -> bool {
        mouths.iter().any(|mouth| Vec2::new(mouth.x - feet.x, mouth.z - feet.z).length() < 0.5)
    }

    /// A Zombie and a Napalm in the closing Aisle are Shoved unharmed when the Roll starts: no death, no burst, no kill credit, no Tip.
    #[test]
    fn a_zombie_and_a_napalm_in_the_closing_aisle_are_shoved_unharmed() {
        let plain: fn(&mut Sim, Vec3) -> Entity = |sim, at| sim.spawn(at, None);
        let napalm: fn(&mut Sim, Vec3) -> Entity = |sim, at| {
            let napalm = sim.spawn(at, None);
            sim.world_mut().entity_mut(napalm).insert(Zombie::special(SpecialKind::Napalm, 1000.0, at));
            napalm
        };
        let (mut sim, bodies, mouths) = shoved_by_a_roll(&[plain, napalm]);
        for body in bodies {
            assert!(sim.world_mut().get::<Dying>(body).is_none(), "not killed");
            assert!(sim.world_mut().get::<Tipped>(body).is_none(), "not Tipped");
            let feet = feet_of_zombie(&mut sim, body);
            assert!(at_a_mouth(feet, &mouths), "{feet} is at a mouth of the Aisle that closed: {mouths:?}");
        }
        assert!(sim.world_mut().resource::<Kills>().0.is_empty(), "no kill, so no credit and no Napalm burst");
    }

    /// A Hellhound and an Elite are Shoved just the same.
    #[test]
    fn a_hellhound_and_an_elite_in_the_closing_aisle_are_shoved() {
        let hound: fn(&mut Sim, Vec3) -> Entity = |sim, at| {
            let hound = sim.spawn(at, None);
            sim.world_mut().entity_mut(hound).insert(Zombie::hellhound(1000.0, at));
            hound
        };
        let elite: fn(&mut Sim, Vec3) -> Entity = |sim, at| {
            let elite = sim.spawn(at, None);
            sim.world_mut().entity_mut(elite).insert(crate::run::gauntlet::Elite(crate::run::gauntlet::EliteTrait::Splitter));
            elite
        };
        let (mut sim, bodies, mouths) = shoved_by_a_roll(&[hound, elite]);
        for body in bodies {
            assert!(sim.world_mut().get::<Dying>(body).is_none());
            let feet = feet_of_zombie(&mut sim, body);
            assert!(at_a_mouth(feet, &mouths), "{feet}: {mouths:?}");
        }
        assert!(sim.world_mut().resource::<Kills>().0.is_empty());
    }

    /// A Zombie that is not in the sweep is left where it stands.
    #[test]
    fn a_zombie_outside_the_sweep_is_not_shoved() {
        let mut sim = archive_sim();
        move_player(&mut sim, Vec3::new(-30.0, 8.0, 30.0));
        let outside = Vec3::new(0.0, 8.0, 16.5);
        let zombie = sim.spawn(outside, None);
        sim.send(SwingStarted { piece: PieceId(0), from: 2, to: 0 });
        sim.run(3);
        assert!(feet_of_zombie(&mut sim, zombie).distance(outside) < 0.3);
    }

    /// A Player who steps into the sweep after the Roll has started is moved to a mouth by the Host's `Teleport`, Down or not.
    #[test]
    fn a_player_who_steps_in_mid_roll_is_moved_to_a_mouth() {
        for down in [false, true] {
            let mut sim = archive_sim();
            sim.run(3);
            // Under way: the Roll from Rest 2 to Rest 0 is a second in.
            show(&mut sim, 0, PiecePhase::Swinging { from: 2, to: 0, elapsed: 1.0 });
            move_player(&mut sim, Vec3::new(0.0, 8.0, 12.5));
            if down {
                let world = sim.world_mut();
                let player = world.query_filtered::<Entity, With<Player>>().iter(world).next().unwrap();
                world.entity_mut(player).insert(crate::players::last_stand::LastStand::shown(10.0, false));
            }
            sim.run(3);
            let world = sim.world_mut();
            let mut query = world.query_filtered::<(Entity, &Transform), With<Player>>();
            let (player, transform) = query.iter(world).next().map(|(entity, transform)| (entity, *transform)).unwrap();
            let feet = transform.translation - Vec3::Y * Stance::Standing.half_height();
            let graph = NavGraph::new(MapId::Archive);
            let mouths: Vec<Vec3> = (0..5).flat_map(|rest| mouths(&graph, PieceId(0), rest)).collect();
            assert!(mouths.iter().any(|mouth| Vec2::new(mouth.x - feet.x, mouth.z - feet.z).length() < 0.3), "down {down}: {feet} should be at one of {mouths:?}");
            assert!(feet.z > 14.5, "on the side they came from: {feet}");
            assert!(world.resource::<Teleported>().0.contains(&player), "the Host told the Player's machine with a Teleport");
        }
    }

    /// A Zombie that walks into the sweep mid-Roll is cut to a mouth too, and unharmed.
    #[test]
    fn a_zombie_that_enters_the_sweep_mid_roll_is_shoved_to_a_mouth() {
        let mut sim = archive_sim();
        move_player(&mut sim, Vec3::new(-30.0, 8.0, 30.0));
        sim.run(3);
        show(&mut sim, 0, PiecePhase::Swinging { from: 2, to: 0, elapsed: 1.0 });
        let zombie = sim.spawn(Vec3::new(-0.2, 8.0, 12.5), None);
        sim.run(3);
        let feet = feet_of_zombie(&mut sim, zombie);
        assert!(feet.z > 14.5, "{feet}");
        assert!(sim.world_mut().get::<Dying>(zombie).is_none());
        assert!(sim.world_mut().resource::<Kills>().0.is_empty());
    }

    // ---- Shoves: where ----

    /// A body in the Aisle that closes goes to the nearer of that Aisle's two mouths, on the side it stands.
    #[test]
    fn a_shove_lands_on_the_nearer_mouth_of_the_rest_it_was_on() {
        let graph = NavGraph::new(MapId::Archive);
        let piece = &MapId::Archive.layout().shifting[0];
        let id = PieceId(0);
        for rest in 0..5u8 {
            let ends = mouths(&graph, id, rest);
            assert_eq!(ends.len(), 2, "Rest {rest} has two mouths");
            // The Range's line is z 12: the mouths are 3.1 m either side of it.
            let (low, high) = if ends[0].z < ends[1].z { (ends[0], ends[1]) } else { (ends[1], ends[0]) };
            assert!((high.z - low.z - 6.2).abs() < 0.01, "{low} {high}");
            let middle = (low + high) / 2.0;
            for (dz, want) in [(-1.5, low), (-0.2, low), (0.2, high), (1.5, high)] {
                let feet = middle + Vec3::new(0.0, 0.0, dz);
                assert_eq!(shove_landing_at_start(&graph, piece, id, rest, feet), Some(want), "Rest {rest}, {dz} m off the line");
            }
        }
        // A Roll's start looks only at the Rest the body was on, though another Rest's mouth may stand nearer.
        let (mid, west) = (mouths(&graph, id, 2), mouths(&graph, id, 0));
        let feet = Vec3::new(west[0].x, 8.0, 12.4);
        let landing = shove_landing(&graph, id, &[2, 0], feet).unwrap();
        assert!(west.contains(&landing), "the nearest of both Aisles");
        assert!(mid.contains(&shove_landing_at_start(&graph, piece, id, 2, Vec3::new(mid[0].x + 0.9, 8.0, 12.4)).unwrap()), "at the start, the Aisle it stood in");
        // In the margin beyond the Bookcases' ends, far along the rail from that Aisle, the nearest mouth of any Aisle instead.
        let margin = Vec3::new(west[0].x, 8.0, 14.7);
        assert!(west.contains(&shove_landing_at_start(&graph, piece, id, 2, margin).unwrap()));
        assert_eq!(shove_landing(&graph, id, &[], feet), None);
    }

    /// No body of any Range is carried farther than the Bookcase's 2.5 m half length and the 0.6 m of clear floor the mouth stands off,
    /// across the line (3.1 m), nor farther along it than the Aisle's half width: from any standing place in any closing Aisle, at every Rest
    /// of every Range. A body cannot clear the shelves in less than the 2.5 m, so that is the 2.5 m of the Shove.
    #[test]
    fn a_shove_moves_a_zombie_at_most_2_5_m() {
        let graph = NavGraph::new(MapId::Archive);
        let layout = MapId::Archive.layout();
        let mut worst = 0.0_f32;
        let mut tried = 0;
        for (id, piece) in pieces(layout) {
            let Motion::Rail(rail) = piece.leaves[0].motion else { panic!("a Range's Leaves are on a rail") };
            let across = Vec2::new(-rail.y, rail.x);
            let hinge = piece.leaves[0].hinge;
            for from in 0..piece.rests.len() as u8 {
                for to in (0..piece.rests.len() as u8).filter(|&to| to != from) {
                    // Every standing place on a 0.25 m grid around the Range: in the sweep, and clear of the Bookcases where they stand.
                    let mut x = hinge.x - 8.0;
                    while x < hinge.x + 8.0 {
                        let mut z = hinge.z - 8.0;
                        while z < hinge.z + 8.0 {
                            let feet = Vec3::new(x, hinge.y, z);
                            z += 0.25;
                            let stood_on = piece.leaves.iter().zip(piece.rests[usize::from(from)].at).any(|(leaf, &at)| leaf.over(feet, at, 0.0));
                            if stood_on || !piece.in_way(feet, from, to) {
                                continue;
                            }
                            let landing = shove_landing_at_start(&graph, piece, id, from, feet).unwrap();
                            let moved = Vec2::new(landing.x - feet.x, landing.z - feet.z);
                            assert!(moved.dot(across).abs() <= 2.5 + 0.6 + 0.01, "{id:?} {from}->{to}: {feet} is carried {} m across the line", moved.dot(across).abs());
                            worst = worst.max(moved.length());
                            tried += 1;
                        }
                        x += 0.25;
                    }
                }
            }
        }
        assert!(tried > 1000, "{tried} standing places");
        assert!(worst <= 3.1_f32.hypot(AISLE_WIDTH / 2.0 + SWING_MARGIN) + 0.01, "{worst} m");
    }

    /// A Wheel use rolls the Great Range, and a Zombie that wants the Player on the far side replans through the Aisle that opened.
    #[test]
    fn zombies_replan_through_the_aisle_a_wheel_opens() {
        let mut sim = archive_sim();
        open_every_door(&mut sim);
        // The Player waits in the Reading Room's west, the Zombie in the Catalogue Hall's, both on the Aisle's far side from the middle.
        let player = Vec3::new(-3.2, 8.0, 20.0);
        move_player(&mut sim, player);
        sim.run(30);
        pull_wheel(&mut sim, 0, 0, 0);
        // It crosses the Range's line (z 12) only once the Bookcases have rolled, and then through the Aisle that opened (x -3.9 to -0.9).
        let zombie = sim.spawn(Vec3::new(-16.0, 8.0, 1.0), None);
        let (mut crossed, mut farthest) = (None, 0.0_f32);
        for _ in 0..sim.frames(60.0) {
            sim.run(1);
            let Some(at) = sim.world_mut().get::<Transform>(zombie).map(|t| t.translation) else { break };
            farthest = farthest.max(at.z);
            if at.z > 12.0 && crossed.is_none() {
                crossed = Some((at, phase(&mut sim, 0)));
            }
        }
        let (at, then) = crossed.expect("the Zombie crossed the Great Range");
        assert!(matches!(then, PiecePhase::Cooling { at: 0, .. } | PiecePhase::Rest(0)), "it waited for the roll to land, and crossed at {at} with the Range {then:?}");
        assert!(at.x > -3.9 && at.x < -0.9, "through the new Aisle: {at}");
        assert!(farthest > 15.0, "and went on to the Player: {farthest}");
    }

    // ---- Lock Gates ----

    const INLAND: PieceId = PieceId(4);
    const SEA: PieceId = PieceId(5);

    fn gates() -> [&'static Piece; 2] {
        [&polder().shifting[4], &polder().shifting[5]]
    }

    #[test]
    fn the_gates_stand_at_their_rest_per_phase_and_exactly_one_top_is_shut_at_each() {
        let [inland, sea] = gates();
        let rests = |piece: &Piece| TIDE_PHASES.map(|phase| piece.tide_rest(phase).unwrap());
        // Rest 0 is Shut, 1 is Open: the Inland Gate is Shut at Low and Rising, the Sea Gate at
        // High and Falling.
        assert_eq!(rests(inland), [0, 0, 1, 1]);
        assert_eq!(rests(sea), [1, 1, 0, 0]);
        for phase in TIDE_PHASES {
            let shut = gates().iter().filter(|piece| piece.tide_rest(phase) == Some(0)).count();
            assert_eq!(shut, 1, "{phase:?}: exactly one gate top is a walkway to the island");
        }
        assert_eq!((inland.start, sea.start), (0, 1), "a Run starts at Low");
        assert!(gates().iter().all(|piece| piece.levers().is_empty() && piece.door.is_some()));
        assert!(market().tide_rest(TidePhase::High).is_none(), "a bridge has no Tide Rest");
        assert_eq!(SWING_SECS, TIDE_TURN_SECS, "a gate swings for as long as the water turns");
    }

    /// Shut, the two Leaves meet mid-chamber with a 1 m walkway between their rails; Open, they lie
    /// inside the chamber against its walls.
    #[test]
    fn a_shut_gate_is_a_one_metre_walkway_and_an_open_one_lies_in_its_chamber() {
        let (inland, sea) = (gates()[0], gates()[1]);
        for (piece, chamber_x, z) in [(inland, (-13.0, -6.0), 2.5), (sea, (6.0, 13.0), 21.5)] {
            let [west, east] = [piece.leaves[0], piece.leaves[1]];
            let shut = piece.rests[0].at;
            // They meet in the middle of the chamber.
            let (a, b) = (west.centre(shut[0]), east.centre(shut[1]));
            assert!((a.x - b.x).abs() > 3.4 && (a.x - b.x).abs() < 3.6, "{a} {b}");
            assert!((a.z - z).abs() < 1e-4 && (b.z - z).abs() < 1e-4, "centred on the wicket's line");
            let mid = (chamber_x.0 + chamber_x.1) / 2.0;
            for x in [mid - 1.0, mid, mid + 1.0] {
                let at = at(x, 0.0, z);
                assert!(west.over(at, shut[0], 0.0) || east.over(at, shut[1], 0.0), "the walkway crosses {x}");
            }
            assert!(!west.over(at(mid, 0.0, z + 0.9), shut[0], 0.0), "the deck is 1.4 m across, rails and all");
            assert!((west.size.x - 2.0 * LEAF_RAIL_THICKNESS - 1.0).abs() < 1e-5, "a 1 m walkway between the rails");
            assert_eq!(west.top(), 0.0, "level with the quay");
            // Open, every part of both Leaves is inside the chamber's own x range.
            for (leaf, &yaw) in piece.leaves.iter().zip(piece.rests[1].at) {
                let centre = leaf.centre(yaw);
                assert!(centre.x > chamber_x.0 + 0.1 && centre.x < chamber_x.1 - 0.1, "folded in the chamber: {centre}");
                assert!(centre.z > 0.0 && centre.z < 24.0);
                assert!(!leaf.over(at(mid, 0.0, z), yaw, 0.0), "Open, the middle is clear");
            }
        }
    }

    fn at(x: f32, y: f32, z: f32) -> Vec3 {
        Vec3::new(x, y, z)
    }

    /// The Polder's headline check. With every Door open and the Power on, in all 16 combinations
    /// of the bridges' Rests at each of the four Tide phases, with each gate at its phase's Rest:
    /// every dry node reaches the Town Square's, and Pack-a-Punch and the Sluice House are among
    /// them. With both gates Swinging, the island is cut off.
    #[test]
    fn pack_a_punch_is_reachable_and_no_zone_is_cut_off_in_all_sixty_four_combinations() {
        let mut graph = NavGraph::new(MapId::Polder);
        let doors = all_doors();
        let yes = |_: crate::maps::map::BarrierId| true;
        let start = graph.nearest(polder().seat_start(0));
        let pap: Vec<usize> = ["pap_foot", "pap_top", "pap_p"].iter().map(|name| graph.node(name).unwrap()).collect();
        let island: Vec<usize> = graph.nodes.iter().enumerate().filter(|(_, node)| node.zone == Some(ZoneId::SluiceHouse)).map(|(i, _)| i).collect();
        let mut checked = 0;
        for phase in TIDE_PHASES {
            graph.tide = phase.level();
            for combo in 0..16u8 {
                let mut rests: Vec<Option<u8>> = (0..4).map(|i| Some((combo >> i) & 1)).collect();
                rests.extend(gates().iter().map(|piece| piece.tide_rest(phase)));
                let state = NavState::new(&doors, &yes).with_rests(&rests);
                for (to, node) in graph.nodes.iter().enumerate() {
                    // Dry ground only: a ford's berm and channel are under water at some phases.
                    if node.zone.is_none() || node.vent || node.name.ends_with("_out") || node.at.y < -0.01 {
                        continue;
                    }
                    assert!(graph.plan(to, start, state).is_some(), "{} is cut off at {phase:?} with bridges {combo:04b}", node.name);
                    checked += 1;
                }
                for &node in &pap {
                    assert!(graph.plan(node, start, state).is_some(), "Pack-a-Punch is cut off at {phase:?} with bridges {combo:04b}");
                }
                assert!(island.iter().all(|&node| graph.plan(node, start, state).is_some()), "the island at {phase:?} {combo:04b}");
            }
            // With both gates Swinging, the Sluice House is unreachable.
            for combo in 0..16u8 {
                let mut rests: Vec<Option<u8>> = (0..4).map(|i| Some((combo >> i) & 1)).collect();
                rests.extend([None, None]);
                let state = NavState::new(&doors, &yes).with_rests(&rests);
                for &node in pap.iter().chain(&island) {
                    assert!(graph.plan(node, start, state).is_none(), "{} is reachable with both gates Swinging ({phase:?}, {combo:04b})", graph.nodes[node].name);
                }
            }
        }
        assert!(checked > 64 * 300, "{checked}");
    }

    /// At High a Zombie reaches the island over the Sea Gate and not the Inland Gate; at Low the
    /// other way round.
    #[test]
    fn a_zombie_reaches_the_island_over_whichever_gate_is_shut() {
        let mut graph = NavGraph::new(MapId::Polder);
        let doors = all_doors();
        let yes = |_: crate::maps::map::BarrierId| true;
        let start = graph.nearest(polder().seat_start(0));
        let (landing, inland_w, inland_b, sea_w, sea_b) = (
            graph.node("pap_foot").unwrap(),
            graph.node("pd1_w").unwrap(),
            graph.node("pd1_b").unwrap(),
            graph.node("pd2_w").unwrap(),
            graph.node("pd2_b").unwrap(),
        );
        for (phase, over_sea) in [(TidePhase::Low, false), (TidePhase::Rising, false), (TidePhase::High, true), (TidePhase::Falling, true)] {
            graph.tide = phase.level();
            let rests: Vec<Option<u8>> = [Some(0); 4].into_iter().chain(gates().iter().map(|piece| piece.tide_rest(phase))).collect();
            let path = graph.plan(start, landing, NavState::new(&doors, &yes).with_rests(&rests)).unwrap();
            let crosses = |from, to| path.windows(2).any(|pair| pair == [from, to]);
            assert_eq!((crosses(sea_w, sea_b), crosses(inland_w, inland_b)), (over_sea, !over_sea), "{phase:?}");
        }
    }

    // ---- Headless ----

    fn set_tide(sim: &mut Sim, phase: TidePhase, turn: TurnState) {
        *sim.world_mut().resource_mut::<Tide>() = Tide { phase, turn };
    }

    /// The Turn from `phase`, run on by hand as `TidePlugin` would: Turning for `TIDE_TURN_SECS`,
    /// then it lands. Returns every Swing the gates began.
    fn turn_from(sim: &mut Sim, phase: TidePhase) -> Vec<(PieceId, u8, u8)> {
        let seen = |sim: &mut Sim| -> Vec<(PieceId, u8, u8)> { std::mem::take(&mut sim.world_mut().resource_mut::<Swings>().0) };
        let mut swings = Vec::new();
        set_tide(sim, phase, TurnState::Turning { elapsed: 0.0 });
        let frames = sim.frames(TIDE_TURN_SECS);
        let dt = sim.frames(1.0) as f32;
        for i in 0..frames {
            sim.run(1);
            let elapsed = (i + 1) as f32 / dt;
            set_tide(sim, phase, TurnState::Turning { elapsed: elapsed.min(TIDE_TURN_SECS) });
            swings.extend(seen(sim));
        }
        set_tide(sim, phase.next(), TurnState::Still);
        sim.run(3);
        swings.extend(seen(sim));
        swings
    }

    #[derive(Resource, Default)]
    struct Swings(Vec<(PieceId, u8, u8)>);

    fn log_swings(mut swings: ResMut<Swings>, mut heard: MessageReader<SwingStarted>) {
        swings.0.extend(heard.read().map(|swing| (swing.piece, swing.from, swing.to)));
    }

    fn gate_sim() -> Sim {
        polder_sim()
    }

    #[test]
    fn the_gates_swing_on_the_rising_to_high_and_falling_to_low_turns_and_on_no_others() {
        let mut sim = gate_sim();
        move_player(&mut sim, Vec3::new(-30.0, 0.0, -30.0));
        let mut log = Vec::new();
        for phase in [TidePhase::Low, TidePhase::Rising, TidePhase::High, TidePhase::Falling] {
            let swings = turn_from(&mut sim, phase);
            log.push((phase, swings));
            // The gates are at the new phase's Rest once it lands.
            for (id, piece) in pieces(polder()).skip(4) {
                assert_eq!(phase_of(&mut sim, id.0), PiecePhase::Rest(piece.tide_rest(phase.next()).unwrap()), "{id:?} after {phase:?}");
            }
        }
        assert!(log[0].1.is_empty(), "Low to Rising: {:?}", log[0].1);
        assert_eq!(log[1].1, [(INLAND, 0, 1), (SEA, 1, 0)], "Rising to High: both gates");
        assert!(log[2].1.is_empty(), "High to Falling: {:?}", log[2].1);
        assert_eq!(log[3].1, [(INLAND, 1, 0), (SEA, 0, 1)], "Falling to Low: both gates");
    }

    fn phase_of(sim: &mut Sim, id: u8) -> PiecePhase {
        phase(sim, id)
    }

    /// A gate swings from the start of Turning over the Turn's seconds, and isn't moving before or
    /// after. It has no cooldown: it is at Rest the moment it lands.
    #[test]
    fn a_gate_swings_through_the_turn_and_is_at_rest_when_it_lands() {
        let mut sim = gate_sim();
        move_player(&mut sim, Vec3::new(-30.0, 0.0, -30.0));
        set_tide(&mut sim, TidePhase::Rising, TurnState::Bells { elapsed: 1.0 });
        sim.run(10);
        assert_eq!(phase(&mut sim, 4), PiecePhase::Rest(0), "the bells move nothing");
        set_tide(&mut sim, TidePhase::Rising, TurnState::Turning { elapsed: 0.0 });
        sim.run(2);
        assert!(matches!(phase(&mut sim, 4), PiecePhase::Swinging { from: 0, to: 1, .. }), "{:?}", phase(&mut sim, 4));
        sim.run(sim.frames(TIDE_TURN_SECS / 2.0));
        let half = leaf_rotation(&mut sim, 4);
        assert!(half != Quat::from_rotation_y(FRAC_PI_2) && half != Quat::IDENTITY, "mid-swing: {half}");
        sim.run(sim.frames(TIDE_TURN_SECS / 2.0) + 10);
        assert_eq!(phase(&mut sim, 4), PiecePhase::Rest(1), "no cooldown, no Lever: straight to Rest");
        sim.run(30);
        assert_eq!(leaf_rotation(&mut sim, 4), Quat::IDENTITY, "lands exactly Open");
        // Nothing a Lever or a Sapper could use.
        let world = sim.world_mut();
        assert_eq!(world.query::<&Lever>().iter(world).filter(|lever| lever.piece.0 >= 4).count(), 0);
    }

    /// The Tide finishing at once (the next Round started) puts the gates at the new Rest.
    #[test]
    fn a_turn_finished_at_once_puts_the_gates_at_the_new_rest() {
        let mut sim = gate_sim();
        move_player(&mut sim, Vec3::new(-30.0, 0.0, -30.0));
        set_tide(&mut sim, TidePhase::High, TurnState::Still);
        sim.run(3);
        assert_eq!(phase(&mut sim, 4), PiecePhase::Rest(1));
        assert_eq!(phase(&mut sim, 5), PiecePhase::Rest(0));
        assert_eq!(sim.world_mut().resource::<Swings>().0, [(INLAND, 0, 1), (SEA, 1, 0)]);
    }

    /// A Player on the Inland Gate's top when it starts to Swing is Washed Up to the Lock Yard's
    /// landing, with no damage and nothing else moved.
    #[test]
    fn a_player_on_the_inland_gate_at_rising_to_high_is_washed_up_to_the_lock_yards_landing() {
        let mut sim = gate_sim();
        move_player(&mut sim, Vec3::new(-11.0, 0.0, 2.5));
        sim.run(3);
        set_tide(&mut sim, TidePhase::Rising, TurnState::Turning { elapsed: 0.0 });
        sim.run(3);
        let world = sim.world_mut();
        let mut query = world.query_filtered::<&Transform, With<Player>>();
        let half = Stance::Standing.half_height();
        let feet = query.iter(world).next().unwrap().translation - Vec3::Y * half;
        let landing = NavGraph::new(MapId::Polder).nodes[NavGraph::new(MapId::Polder).node("pd1_w").unwrap()].at;
        assert!(feet.distance(landing) < 0.3, "{feet} vs the Lock Yard's landing {landing}");
        assert_eq!(zone_of_feet(feet), Some(ZoneId::LockYard));
    }

    fn zone_of_feet(feet: Vec3) -> Option<ZoneId> {
        crate::enemies::nav::zone_at(MapId::Polder, feet + Vec3::Y * 0.1)
    }

    /// One nearer the island goes to the island's landing, and one in the sweep of a gate that is
    /// not moving is left where they are.
    #[test]
    fn a_player_nearer_the_island_is_washed_up_to_its_landing_and_a_gate_that_stays_leaves_them() {
        let mut sim = gate_sim();
        move_player(&mut sim, Vec3::new(-7.5, 0.0, 2.5));
        sim.run(3);
        set_tide(&mut sim, TidePhase::Rising, TurnState::Turning { elapsed: 0.0 });
        sim.run(3);
        let island = Vec3::new(-4.0, 0.0, 2.5);
        let world = sim.world_mut();
        let mut query = world.query_filtered::<&Transform, With<Player>>();
        let feet = query.iter(world).next().unwrap().translation - Vec3::Y * Stance::Standing.half_height();
        assert!(feet.distance(island) < 0.3, "{feet}");
        // Low to Rising moves nothing, so nobody is Washed Up.
        let mut sim = gate_sim();
        let there = Vec3::new(-11.0, 0.0, 2.5);
        move_player(&mut sim, there);
        sim.run(3);
        set_tide(&mut sim, TidePhase::Low, TurnState::Turning { elapsed: 0.0 });
        sim.run(10);
        let world = sim.world_mut();
        let mut query = world.query_filtered::<&Transform, With<Player>>();
        let feet = query.iter(world).next().unwrap().translation - Vec3::Y * Stance::Standing.half_height();
        assert!(feet.distance(there) < 0.1, "{feet}");
    }

    /// A Napalm carried over onto a gate is Tipped: it dies as an Unpaid Kill by the Swing, which
    /// the specials' death rules treat as put out, with no explosion.
    #[test]
    fn a_napalm_on_a_gate_is_tipped_by_the_swing() {
        let mut sim = gate_sim();
        move_player(&mut sim, Vec3::new(-30.0, 0.0, -30.0));
        let on_gate = Vec3::new(-11.0, 0.0, 2.5);
        let napalm = sim.spawn(on_gate, None);
        sim.world_mut().entity_mut(napalm).insert(Zombie::special(SpecialKind::Napalm, 1000.0, on_gate));
        set_tide(&mut sim, TidePhase::Rising, TurnState::Turning { elapsed: 0.0 });
        sim.run(4);
        let world = sim.world_mut();
        let dying = world.get::<Dying>(napalm).expect("tipped");
        assert_eq!(dying.killed_by(), Some(Attack::Swing));
        assert!(world.get::<Tipped>(napalm).is_some());
        assert_eq!(world.resource::<Kills>().0, [(Attack::Swing, Pay::Unpaid)]);
    }

    /// With the gate's Door open and the gate Shut, the Sluice House is an Active Zone from the Lock
    /// Yard, and not once the gate is Open or moving.
    #[test]
    fn the_sluice_house_is_active_only_over_a_shut_gate_with_its_wicket_open() {
        use crate::maps::door::Door;
        let mut sim = polder_sim();
        let active = |sim: &mut Sim| {
            sim.world_mut().run_system_once(update_active_zones).unwrap();
            sim.world_mut().resource::<ActiveZones>().0
        };
        let world = sim.world_mut();
        world.query::<&mut PlayerZone>().iter_mut(world).next().unwrap().0 = LockYard;
        assert!(!active(&mut sim).contains(SluiceHouse), "the wicket is shut");
        let mut doors = sim.world_mut().query::<&mut Door>();
        for mut door in doors.iter_mut(sim.world_mut()) {
            if door.gate == Some((LockYard, SluiceHouse)) {
                assert_eq!((door.joins, door.price), (None, None), "a Power Door that joins nothing");
                door.open = true;
            }
        }
        assert!(active(&mut sim).contains(SluiceHouse), "Shut at the start");
        show(&mut sim, 4, PiecePhase::Rest(1));
        assert!(!active(&mut sim).contains(SluiceHouse), "Open");
        show(&mut sim, 4, PiecePhase::Swinging { from: 0, to: 1, elapsed: 1.0 });
        assert!(!active(&mut sim).contains(SluiceHouse), "none count while it moves");
    }

    /// At every phase, the gate that is Shut has no Deep wall across its mouth, and the lock's
    /// water is Deep under both walkways.
    #[test]
    fn the_lock_water_is_deep_under_both_gate_mouths_at_every_phase() {
        for phase in TIDE_PHASES {
            for (x, z) in [(-9.5, 2.5), (9.5, 21.5), (-9.5, 12.0), (9.5, 12.0)] {
                assert_eq!(water_at(polder(), phase.level(), Vec3::new(x, -3.0, z)), Some(Wading::Deep), "{x}, {z} at {phase:?}");
            }
        }
    }

}
