//! Shifting: Pieces of the world that turn about vertical hinges between fixed Rests, and the
//! Levers or the Tide that drive them. The Polder's four Swing Bridges are turned by Levers; its
//! two Lock Gates by the Tide, which gives each the Rest it stands at in every phase and swings it
//! along with the water.
//!
//! A Piece is one or more Leaves, each a solid box (a deck with rails) that turns about its own
//! vertical hinge. The Host decides everything: a Lever pull starts a cycle of Warning, Waiting
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
use crate::maps::tide::{Tide, TidePhase, TideTurn, TurnState};
use crate::maps::zone::ActiveZones;
use crate::maps::zone::ZoneId;
use crate::players::player::{Locomotion, LocalPlayer, Player, Stance, Teleport};
use crate::state::{InRun, world_runs};
use crate::tuning::*;

/// A Piece's place in `Layout.shifting`, the same on every machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PieceId(pub u8);

/// One solid box that turns about a vertical hinge: a deck with its rails, or a gate leaf.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Leaf {
    /// The hinge's spot in the world; its height is the Leaf's datum.
    pub hinge: Vec3,
    /// The box's full size.
    pub size: Vec3,
    /// The box's centre from the hinge, with the Leaf at yaw 0.
    pub offset: Vec3,
}

impl Leaf {
    /// Where the Leaf's centre is at `yaw`.
    pub fn centre(&self, yaw: f32) -> Vec3 {
        self.hinge + Quat::from_rotation_y(yaw) * self.offset
    }

    /// The height of the top of its deck.
    pub fn top(&self) -> f32 {
        self.hinge.y + self.offset.y + self.size.y / 2.0
    }

    /// Whether the point `at` (flat) is over the box turned to `yaw`, `margin` out from its edge.
    pub fn over(&self, at: Vec3, yaw: f32, margin: f32) -> bool {
        let local = Quat::from_rotation_y(-yaw) * (at - self.hinge) - self.offset;
        local.x.abs() <= self.size.x / 2.0 + margin && local.z.abs() <= self.size.z / 2.0 + margin
    }

    /// Whether feet at `feet` are at the heights a body on the deck or in the water below it is.
    pub fn height_in_reach(&self, feet: Vec3) -> bool {
        (self.top() - SWING_REACH_BELOW..=self.top() + SWING_REACH_ABOVE).contains(&feet.y)
    }
}

/// Where a Piece can stand still: one yaw for each Leaf, and the Zone joins it makes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rest {
    pub yaws: &'static [f32],
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

/// The yaw `elapsed` seconds into a Swing from `from` to `to`: a smoothstep that lands exactly.
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

    /// Every Leaf's yaw `elapsed` seconds into a Swing from Rest `from` to Rest `to`.
    pub fn yaws(&self, from: u8, to: u8, elapsed: f32) -> Vec<f32> {
        let (a, b) = (self.rests[usize::from(from)].yaws, self.rests[usize::from(to)].yaws);
        a.iter().zip(b).map(|(&a, &b)| pose(a, b, elapsed)).collect()
    }

    /// Whether feet are on a Leaf or in its sweep (the ground it passes over between two Rests),
    /// at deck height or in the water below it.
    pub fn in_way(&self, feet: Vec3, from: u8, to: u8) -> bool {
        self.leaves.iter().enumerate().any(|(i, leaf)| {
            if !leaf.height_in_reach(feet) {
                return false;
            }
            let (a, b) = (self.rests[usize::from(from)].yaws[i], self.rests[usize::from(to)].yaws[i]);
            let steps = ((b - a).abs() / 0.01).ceil().max(1.0) as u32;
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
            Driver::Tide(_) => &[],
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
            Driver::Levers(_) => None,
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
    pub fn advance(&mut self, dt: f32, clear: bool) -> Beat {
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
                    if elapsed + left < LEVER_COOLDOWN_SECS {
                        *self = PiecePhase::Cooling { at, elapsed: elapsed + left };
                        return beat;
                    }
                    left -= LEVER_COOLDOWN_SECS - elapsed;
                    *self = PiecePhase::Rest(at);
                }
            }
        }
    }

    /// The yaw of each Leaf of `piece` now.
    pub fn yaws(self, piece: &Piece) -> Vec<f32> {
        match self {
            PiecePhase::Swinging { from, to, elapsed } => piece.yaws(from, to, elapsed),
            other => {
                let at = other.standing().unwrap_or(0);
                piece.rests[usize::from(at)].yaws.to_vec()
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
    /// Whether the Tide drives it: it comes to Rest at the end of a Swing, with no cooldown.
    tide: bool,
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
        self.phase.advance(dt, false);
        self.settle();
    }

    /// A Tide-driven Piece has no cooldown: a landed Swing is at Rest.
    fn settle(&mut self) {
        if self.tide
            && let PiecePhase::Cooling { at, .. } = self.phase
        {
            self.phase = PiecePhase::Rest(at);
        }
    }

    /// A Piece standing at its start Rest.
    pub fn at_start(id: PieceId, piece: &Piece) -> Self {
        Self { id, phase: PiecePhase::Rest(piece.start), activator: None, tide: matches!(piece.driver, Driver::Tide(_)) }
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
    for (id, piece) in pieces(layout) {
        commands.spawn((
            InRun,
            Name::new("Piece"),
            PieceState::at_start(id, piece),
            Transform::from_translation(piece.leaves[0].hinge),
            Visibility::default(),
        ));
        let yaws = piece.rests[usize::from(piece.start)].yaws;
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
                    Transform::from_translation(leaf.hinge).with_rotation(Quat::from_rotation_y(yaws[index])),
                    Visibility::default(),
                ))
                .with_children(|parent| {
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
                let shut = piece.rests[0].yaws[piece.leaves.iter().position(|other| other == leaf).unwrap_or(0)];
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
    }
}

pub struct ShiftingPlugin;

impl Plugin for ShiftingPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SwingStarted>()
            .add_message::<Teleport>()
            .add_systems(FixedUpdate, (pose_leaves, throw_tipped).run_if(world_runs))
            .add_systems(FixedUpdate, move_sappers.in_set(HostSimulation).run_if(world_runs))
            .add_systems(
                Update,
                (
                    (use_levers, drive_gates.after(TideTurn), run_pieces, tip_occupants, wash_gate_players).chain().in_set(HostSimulation),
                    (prompt_levers, light_lamps, light_gate_lamps, ring_cues).chain(),
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
        let rotation = Quat::from_rotation_y(state.phase.yaws(piece)[body.index]);
        if transform.rotation != rotation {
            transform.rotation = rotation;
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
        let beat = state.phase.advance(time.delta_secs(), clear);
        state.settle();
        if let Some((from, to)) = beat.started {
            swings.write(SwingStarted { piece: id, from, to });
        }
    }
}

/// The debug key: the Piece nearest the Player swings to its next Rest now, with no price,
/// warning or cooldown, and still Tips.
pub(crate) fn debug_swing(
    player: Query<&Transform, With<LocalPlayer>>,
    mut pieces: Query<&mut PieceState>,
    map: Res<MapId>,
    mut swings: MessageWriter<SwingStarted>,
) {
    let Ok(player) = player.single() else { return };
    let layout = map.layout();
    let nearest = pieces
        .iter()
        .filter_map(|state| layout.shifting.get(usize::from(state.id.0)).filter(|piece| !matches!(piece.driver, Driver::Tide(_))).map(|piece| (state.id, piece.leaves[0].hinge.distance(player.translation))))
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

/// The clunk at Rest, on every machine, from the phase it shows.
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
        if had && !swinging
            && let Some(piece) = map.layout().shifting.get(usize::from(state.id.0))
        {
            let yaws = state.phase.yaws(piece);
            let at = piece.leaves[0].centre(yaws[0]);
            cues.write(PlayCue::new(Cue::BridgeClunk, Some(at)));
        }
    }
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
        phase.advance(SWING_WARN_SECS - 0.1, true);
        assert!(matches!(phase, PiecePhase::Warning { .. }));
        phase.advance(0.1, false);
        assert_eq!(phase, PiecePhase::Waiting { from: 0, to: 1 });
        // Waiting has no time limit.
        assert_eq!(phase.advance(1000.0, false), Beat::default());
        assert_eq!(phase, PiecePhase::Waiting { from: 0, to: 1 });
        let beat = phase.advance(0.0, true);
        assert_eq!(beat.started, Some((0, 1)));
        assert_eq!(phase, PiecePhase::Swinging { from: 0, to: 1, elapsed: 0.0 });
        assert_eq!(phase.standing(), None);
        let beat = phase.advance(SWING_SECS, true);
        assert_eq!(beat.landed, Some(1));
        assert_eq!(phase, PiecePhase::Cooling { at: 1, elapsed: 0.0 });
        assert_eq!(phase.standing(), Some(1));
        phase.advance(LEVER_COOLDOWN_SECS - 0.1, true);
        assert!(matches!(phase, PiecePhase::Cooling { .. }));
        phase.advance(0.1, true);
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
    fn only_the_polder_has_pieces_four_bridges_each_with_span_and_along_and_two_lock_gates() {
        for map in MapId::ALL {
            assert_eq!(map.layout().shifting.len(), if map == MapId::Polder { 6 } else { 0 }, "{}", map.name());
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
                    for (leaf, &yaw) in piece.leaves.iter().zip(rest.yaws) {
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
                let span = piece.rests[0].yaws[0];
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
            let shut = piece.rests[0].yaws;
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
            for (leaf, &yaw) in piece.leaves.iter().zip(piece.rests[1].yaws) {
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
        let seen = |sim: &mut Sim| -> Vec<(PieceId, u8, u8)> { sim.world_mut().resource_mut::<Swings>().0.drain(..).collect() };
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
