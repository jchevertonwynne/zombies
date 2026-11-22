use std::f32::consts::FRAC_PI_2;
use std::time::Duration;

use avian3d::prelude::*;
use bevy::input::common_conditions::input_just_pressed;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::render::view::ColorGrading;
use bevy::window::{CursorGrabMode, CursorOptions};
use bevy_transform_interpolation::TranslationEasingState;

use crate::audio::PlayCue;
use crate::audio::mix::stance_cue;
use crate::authority::HostSimulation;
use crate::maps::barrier::PlankTally;
use crate::players::controls::{Action, Controls};
use crate::weapons::equipment::Equipment;
use crate::players::last_stand::LastStand;
use crate::players::out_of_bounds::Footing;
use crate::players::respawn::AwaitingRespawn;
use crate::layers::GameLayer;
use crate::maps::jump_pad::JumpPad;
use crate::maps::zipline::{OpenZones, Zipline, ZiplineFocus};
use crate::maps::map::MapId;
use crate::maps::power::Power;
use crate::run::party::Party;
use crate::net::play::Puppet;
use crate::modifier::{Modifier, Modifiers, RunModifiers, Stat, resolve};
use crate::economy::perk::Drinking;
use crate::economy::points::Points;
use crate::weapons::ammo_mod::Cooldowns;
use crate::players::last_stand::SparesUsed;
use crate::players::shellshock::Shellshocked;
use crate::run::stats::RunStats;
use crate::maps::zone::PlayerZone;
use crate::run::round::{Round, RoundPhase};
use crate::storage::settings::{Settings, StanceMode};
use crate::state::{Choosing, GameState, InRun, Pause, input_runs, world_runs};
use crate::players::teammate::{HeldWeapon, Pose, Shots};
use crate::tuning::*;
use crate::weapons::weapon::{Knife, Loadout, StartingLoadout, move_towards};

/// How low a Player's body is: Standing, Crouched behind the Crouch key, Prone behind theirs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Stance {
    #[default]
    Standing,
    Crouched,
    Prone,
}

impl Stance {
    /// The Stance's factor on walk speed. (The Last Stand Crawl has its own.)
    pub fn speed_mult(self) -> f32 {
        match self {
            Stance::Standing => 1.0,
            Stance::Crouched => CROUCH_SPEED_MULT,
            Stance::Prone => PRONE_SPEED_MULT,
        }
    }

    /// How low the Stance sits: 0 Standing, 1 Crouched, 2 Prone. A change to a deeper Stance only
    /// shrinks the body; a change to a shallower one passes the ceiling check first.
    pub(crate) fn depth(self) -> u8 {
        match self {
            Stance::Standing => 0,
            Stance::Crouched => 1,
            Stance::Prone => 2,
        }
    }

    /// Half the height of the Stance's body: how far its centre sits above the feet.
    pub fn half_height(self) -> f32 {
        match self {
            Stance::Standing => PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0,
            Stance::Crouched => PLAYER_RADIUS + CROUCH_COLLIDER_LENGTH / 2.0,
            Stance::Prone => PRONE_COLLIDER_HEIGHT / 2.0,
        }
    }

    /// The eye's height above the feet in this Stance.
    pub fn eye_height(self) -> f32 {
        match self {
            Stance::Standing => PLAYER_EYE_HEIGHT + Stance::Standing.half_height(),
            Stance::Crouched => CROUCH_EYE_HEIGHT,
            Stance::Prone => PRONE_EYE_HEIGHT,
        }
    }

    /// The capsule the Stance's body is. The prone one is the short upright shape with no
    /// length between its caps; the lying body is drawn, never collided with.
    pub fn collider(self) -> Collider {
        match self {
            Stance::Standing => Collider::capsule(PLAYER_RADIUS, PLAYER_CAPSULE_LENGTH),
            Stance::Crouched => Collider::capsule(PLAYER_RADIUS, CROUCH_COLLIDER_LENGTH),
            Stance::Prone => Collider::capsule(PLAYER_RADIUS, PRONE_COLLIDER_HEIGHT - 2.0 * PLAYER_RADIUS),
        }
    }
}

/// The Crouch and Prone keys resolved into the Stance the Player wants, and the memory behind
/// it. `fill_move_input` runs it for the `LocalPlayer` each tick, by the Stance Keys setting. The
/// Toggle or Hold choice never crosses the wire: only the resolved target rides the `MoveInput`,
/// so the Host replays the same presses to the same Stance.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct StanceKeys {
    /// The Stance wanted.
    pub target: Stance,
    /// The Crouch, Prone, Jump and Sprint keys last tick, for edges.
    held: [bool; 4],
    /// A Jump stand-up: the Stance keys give way until they are released, and the jump that stood
    /// the Player up doesn't go on to jump them.
    yielded: bool,
    /// A jump that stood the Player up, still held.
    standing_jump: bool,
}

impl Default for StanceKeys {
    fn default() -> Self {
        Self { target: Stance::Standing, held: [false; 4], yielded: false, standing_jump: false }
    }
}

impl StanceKeys {
    /// The Stance wanted comes back Standing, as a forced stand-up (a Respawn, a Teleporter
    /// ride) resets it to.
    pub(crate) fn stood_up(&mut self) {
        self.target = Stance::Standing;
        self.yielded = false;
        self.standing_jump = false;
    }

    /// A forced stand-up — a Respawn, a Teleporter ride — on a Player whatever game they're in:
    /// the body Standing at once and, for the `LocalPlayer`, the Stance wanted too. On a client
    /// it rides the reliable event that told of it, so the Stance keys stop asking the Host for
    /// the Stance the Player was in; the next snapshot puts the body where the Host has it.
    pub(crate) fn force_stand(locomotion: &mut Locomotion, collider: &mut Collider, keys: Option<Mut<StanceKeys>>) {
        if let Some(mut keys) = keys {
            keys.stood_up();
        }
        locomotion.stand();
        *collider = Stance::Standing.collider();
    }

    /// One tick of the Crouch, Prone, Jump and Sprint keys into the Stance wanted, given whether
    /// the Player is `grounded` and `down`. Returns whether the jump passes this tick: a jump that
    /// stands the Player up, or one held from it, doesn't jump them.
    ///
    /// Stance input is dead while Down: the target comes back Standing, and a Hold key held
    /// through it must be released and pressed again once up. Airborne presses are
    /// ignored; a Hold key still held lands the Player in its Stance when they do.
    pub fn resolve(
        &mut self,
        mode: StanceMode,
        crouch: bool,
        prone: bool,
        jump: bool,
        sprint: bool,
        grounded: bool,
        down: bool,
    ) -> bool {
        let [crouch_was, prone_was, jump_was, sprint_was] = self.held;
        self.held = [crouch, prone, jump, sprint];
        if down {
            // A Hold key held through the Down gives way too, so the get-up ends Standing: no
            // route back to Crouched or Prone through a Down.
            self.target = Stance::Standing;
            self.yielded = true;
            return jump;
        }
        if !jump {
            self.standing_jump = false;
        }
        match mode {
            StanceMode::Toggle => {
                if grounded {
                    // Crouch switches Standing↔Crouched, from Prone to Crouched; Prone switches
                    // Standing↔Prone, from Crouched to Prone.
                    if prone && !prone_was {
                        self.target = if self.target == Stance::Prone { Stance::Standing } else { Stance::Prone };
                    } else if crouch && !crouch_was {
                        self.target = if self.target == Stance::Crouched { Stance::Standing } else { Stance::Crouched };
                    }
                    // Jump and Sprint stand the Player up instead of jumping or sprinting low.
                    if self.target != Stance::Standing && (jump && !jump_was || sprint && !sprint_was) {
                        self.target = Stance::Standing;
                        self.standing_jump = jump && !jump_was;
                    }
                }
            }
            StanceMode::Hold => {
                if !crouch && !prone {
                    self.yielded = false;
                }
                if grounded && self.target != Stance::Standing && jump && !jump_was {
                    self.yielded = true;
                    self.standing_jump = true;
                }
                if sprint {
                    // Sprint wins over the held key, while it is held.
                    self.target = Stance::Standing;
                } else if self.yielded {
                    self.target = Stance::Standing;
                } else {
                    // A held key keeps the Player in its Stance, Prone over Crouch; letting go
                    // brings them up — out of Prone into Crouched, if Crouch is still held.
                    self.target = if prone {
                        Stance::Prone
                    } else if crouch {
                        Stance::Crouched
                    } else {
                        Stance::Standing
                    };
                }
            }
        }
        jump && !(self.standing_jump && jump)
    }
}

#[derive(Component)]
#[require(HitGrace, Footing)]
pub struct Player;

/// A Player's Hit Grace: when the last Zombie-family melee hit that landed on them stops
/// shielding them from the next, in `Time<Virtual>`'s elapsed time. The Host judges it where
/// Zombie-family hits are sent; a hit it blocks is never sent, so it does nothing at all.
#[derive(Component, Debug, Default)]
pub struct HitGrace {
    ends: Duration,
}

impl HitGrace {
    /// Whether a Zombie-family melee hit at `now` lands. One that lands opens Hit Grace for
    /// `HIT_GRACE_SECS`.
    pub fn try_land(&mut self, now: Duration) -> bool {
        if now < self.ends {
            return false;
        }
        self.ends = now + Duration::from_secs_f32(HIT_GRACE_SECS);
        true
    }
}

/// Which of the four Seats a Player holds for the Run: 0 is the Host's, and solo is always 0. It
/// names the Player wherever an `Entity` can't (across the network), and sets their colour.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Seat(pub u8);

/// Each Seat's colour, in Seat order: white, blue, yellow, green.
pub const SEAT_COLOURS: [Color; 4] = [
    Color::srgb(0.95, 0.95, 0.95),
    Color::srgb(0.35, 0.6, 1.0),
    Color::srgb(1.0, 0.85, 0.25),
    Color::srgb(0.4, 0.9, 0.4),
];

impl Seat {
    pub fn colour(self) -> Color {
        SEAT_COLOURS[usize::from(self.0) % SEAT_COLOURS.len()]
    }
}

/// The name a Player goes by, shown to their teammates.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct PlayerName(pub String);

/// The one Player this machine drives: the only Player solo. The camera, view model and listener
/// hang off it, and the HUD and screen effects read it alone.
#[derive(Component)]
pub struct LocalPlayer;

/// The first-person camera, a child of the Player. It carries the pitch; the Player carries the yaw.
#[derive(Component)]
pub struct PlayerCamera;

/// How high the camera sits above the capsule centre, before any camera sway.
#[derive(Component)]
pub struct Eye {
    pub height: f32,
}

#[derive(Component)]
pub struct Health {
    pub current: f32,
    pub max: f32,
    pub(crate) since_hit: f32,
}

/// What a Player asked for this tick: the one thing `step_move` reads besides their
/// `Locomotion`. The `LocalPlayer`'s is filled from `Controls` each tick; a teammate's will come
/// off the network. The Stance the Player wants arrives resolved: `crouch` and `prone` are the
/// target stance, already past the Toggle or Hold setting (Prone wins if both somehow ride).
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct MoveInput {
    /// Move axes, each -1, 0 or 1: x is right, y is forward.
    pub axes: Vec2,
    pub jump: bool,
    /// The Use key was pressed while a Zipline's Prompt was showing: grab the handle.
    pub grab: bool,
    pub sprint: bool,
    pub aim: bool,
    /// The Player wants the Crouched Stance.
    pub crouch: bool,
    /// The Player wants the Prone Stance.
    pub prone: bool,
    /// The Player flies: no gravity and no collision with the Map. Jump goes up, Crouch down and
    /// Sprint faster. It is the step's own flag, with a meaning outside any layer; the Host ignores
    /// it for a Player who may not (see `net::play::drive_remotes`).
    pub noclip: bool,
    /// Which way the Player faces, in radians about +Y (0 faces -Z).
    pub yaw: f32,
    /// How far the Player looks up (positive) or down, in radians.
    pub pitch: f32,
}

/// The Player has flight on: their `MoveInput` asks for noclip. Whoever turns it on (Developer mode
/// does) inserts it on the `LocalPlayer`; the Host honours the step's bit only where it allows.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct Noclip;

#[derive(Component, Clone)]
pub struct Locomotion {
    pub(crate) velocity: Vec3,
    pub(crate) grounded: bool,
    pub sprinting: bool,
    /// Seconds of sprint left.
    pub(crate) stamina: f32,
    /// Sprint ran out; it can't start again until fully recharged.
    pub(crate) winded: bool,
    /// The Stance the body is shaped in. It changes at the start of a transition, both ways.
    pub stance: Stance,
    /// The Stance the current stance change left. Until the change ends the Player moves at the
    /// slower of it and `stance`, and a change that left Prone keeps Sprint off until it's done.
    pub(crate) left_stance: Stance,
    /// Seconds the current stance change takes: `STANCE_CROUCH_SECS`, `STANCE_PRONE_SECS` or
    /// `STANCE_PRONE_FROM_STAND_SECS` by the pair.
    pub(crate) stance_change_secs: f32,
    /// Seconds since the current stance change began; `stance_change_secs` or more once settled.
    pub(crate) since_stance_change: f32,
    /// The eye's height above the feet, gliding between stance eye heights.
    pub(crate) eye: f32,
    /// The speed the eye glides at, set when a stance change begins.
    pub(crate) eye_rate: f32,
    /// A Dive in flight: a ballistic lunge that ends Prone where it lands.
    pub(crate) diving: bool,
    /// In flight off a Jump Pad: thrown on an arc nothing steers, until it lands.
    pub(crate) thrown: bool,
    /// Riding a Zipline: along its straight line at its pace, nothing steering or stopping it,
    /// until it lands. `velocity` is the ride's, along the line.
    pub(crate) riding: bool,
    /// Where the ride lands, kept up to date while riding: a riding Player counts as standing
    /// here for targeting.
    pub(crate) ride_to: Vec3,
    /// Seconds since the last Dive landed, counting through its recovery and its cooldown.
    /// `f32::INFINITY` until the first Dive lands.
    pub(crate) since_dive_landing: f32,
    /// Lying in the Down pose: on the back, in the prone body. Down is not a Stance, so `stance`
    /// reads Standing all the while; the step lays the body down when a Down starts and rises it
    /// when the Down ends.
    pub(crate) down: bool,
    /// Flying: the last step was a noclip one (see `MoveInput::noclip`).
    pub(crate) flying: bool,
    /// The step that ended a flight has run and nothing has put the body back on the Map since:
    /// `catch_out_of_bounds` clears it, and moves the Player to their last legal spot, with no
    /// Down, if they came down Out of Bounds.
    pub(crate) left_flight: bool,
}

impl Locomotion {
    /// Whether the body is flying through the Map (a noclip step).
    pub fn flying(&self) -> bool {
        self.flying
    }

    pub fn grounded(&self) -> bool {
        self.grounded
    }

    /// Whether a stance change is still under way: the camera is still gliding and the Player
    /// still moves at the slower Stance's speed.
    pub(crate) fn changing_stance(&self) -> bool {
        self.since_stance_change < self.stance_change_secs
    }

    /// Whether a prone transition — going down to Prone or getting up from it — is still under
    /// way. Firing is blocked until it's done; crouch transitions block nothing.
    pub fn prone_transition(&self) -> bool {
        self.changing_stance() && (self.stance == Stance::Prone || self.left_stance == Stance::Prone)
    }

    /// The shape the body is in: its Stance's, or while Down the prone one, which the Down
    /// shares — one lying-down collider, eye and look limit, two faces (Prone face-down, the
    /// Down on the back).
    pub fn shape(&self) -> Stance {
        if self.down { Stance::Prone } else { self.stance }
    }

    /// Lays the body down into the Down pose, from whatever it was doing: the stance resets to
    /// Standing, any stance change is over, and a Dive in flight hands over to plain physics —
    /// its recovery lock and cooldown wiped — so the Player lands straight into the pose. The
    /// eye drops to the prone eye at the collapse's speed. Returns how far the body's centre
    /// moves up (down, here) to keep the feet where they are.
    fn collapse(&mut self) -> f32 {
        let lift = Stance::Prone.half_height() - self.shape().half_height();
        // Standing at once, but for the eye, which glides down from where it is.
        let eye = self.eye;
        self.stand();
        self.eye = eye;
        self.eye_rate = collapse_rate();
        self.down = true;
        lift
    }

    /// Gets the body up out of the Down pose into Standing, as a Revive or Self-Revive does: a
    /// timed rise mirroring the collapse, run as a change out of Prone, so the stance rules —
    /// the firing lock, the slower speed, no Sprint — apply until upright. Returns how far the
    /// body's centre moves up.
    fn rise(&mut self) -> f32 {
        self.down = false;
        let lift = Stance::Standing.half_height() - Stance::Prone.half_height();
        self.left_stance = Stance::Prone;
        self.stance = Stance::Standing;
        self.eye_rate = collapse_rate();
        self.stance_change_secs = (Stance::Standing.eye_height() - self.eye).abs() / self.eye_rate;
        self.since_stance_change = 0.0;
        lift
    }

    /// Puts the body in the Standing Stance at once, as a Respawn, a Teleporter ride or the start
    /// of a Run does. The caller sets the collider to match. A Dive in flight is over, its locks
    /// wiped, and so is any Down pose, with no rise to animate.
    pub(crate) fn stand(&mut self) {
        self.down = false;
        self.stance = Stance::Standing;
        self.left_stance = Stance::Standing;
        self.stance_change_secs = 0.0;
        self.since_stance_change = 0.0;
        self.eye = Stance::Standing.eye_height();
        self.eye_rate = 0.0;
        self.diving = false;
        self.since_dive_landing = f32::INFINITY;
    }

    /// Puts the body in the Stance the Host has it in, if this game had it otherwise: the change
    /// glides there as one from the Stance this game had, and Prone ∧ airborne is a Dive in
    /// flight. A Stance both agree on is left alone, mid-change or not. While Down the Stance
    /// reads Standing on both sides, and the Down pose is the reliable events' to say.
    pub(crate) fn heal_stance(&mut self, stance: Stance) {
        if self.stance == stance || self.down {
            return;
        }
        self.left_stance = self.stance;
        self.stance = stance;
        self.stance_change_secs = transition_secs(self.left_stance, stance);
        self.since_stance_change = 0.0;
        self.eye_rate = (stance.eye_height() - self.eye).abs() / self.stance_change_secs;
        self.diving = stance == Stance::Prone && !self.grounded;
    }

    /// Whether a Dive is in flight. With airborne presses ignored there are no crouch-jumps, so
    /// Prone ∧ airborne can only mean a Dive in flight — the wire derives it, never encodes it.
    pub fn diving(&self) -> bool {
        self.diving
    }

    /// What the Player's own stance feedback reads off the body, `last_stand` or not.
    pub fn feel(&self, last_stand: bool) -> StanceFeel {
        StanceFeel {
            shown: (!last_stand && !self.down).then_some(self.stance),
            diving: self.diving(),
        }
    }

    /// Whether the last Dive's landing recovery still has the Player locked: firing, aiming and
    /// stance changes are blocked until `DIVE_RECOVERY_SECS` after landing.
    pub fn recovering_from_dive(&self) -> bool {
        self.since_dive_landing < DIVE_RECOVERY_SECS
    }

    /// Whether the last Dive's cooldown still holds: no new Dive until `DIVE_COOLDOWN_SECS`
    /// after landing. The recovery runs inside it; only the Dive waits on the cooldown.
    pub fn dive_on_cooldown(&self) -> bool {
        self.since_dive_landing < DIVE_COOLDOWN_SECS
    }

    /// Whether a Zipline ride is under way.
    pub fn riding(&self) -> bool {
        self.riding
    }

    /// Where the Zipline ride under way lands, once the step has found the line.
    pub fn landing(&self) -> Option<Vec3> {
        (self.riding && self.ride_to != Vec3::ZERO).then_some(self.ride_to)
    }

    /// Horizontal speed in m/s.
    pub fn ground_speed(&self) -> f32 {
        self.velocity.with_y(0.0).length()
    }

    /// Upward speed in m/s; negative while falling.
    pub fn vertical_speed(&self) -> f32 {
        self.velocity.y
    }
}

impl Default for Locomotion {
    fn default() -> Self {
        Self {
            velocity: Vec3::ZERO,
            grounded: false,
            sprinting: false,
            thrown: false,
            riding: false,
            ride_to: Vec3::ZERO,
            stamina: SPRINT_DURATION_SECS,
            winded: false,
            stance: Stance::Standing,
            left_stance: Stance::Standing,
            stance_change_secs: 0.0,
            since_stance_change: 0.0,
            eye: Stance::Standing.eye_height(),
            eye_rate: 0.0,
            diving: false,
            since_dive_landing: f32::INFINITY,
            down: false,
            flying: false,
            left_flight: false,
        }
    }
}

/// What the local Player's own stance feedback reads off their body: the HUD icon and the stance
/// sounds. The icon follows the body's Stance, which changes at the start of a transition — the
/// same instant the collider and the locks do — and is Prone from a Dive's launch on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StanceFeel {
    /// The Stance the icon shows; none while Down, which is not a Stance.
    pub shown: Option<Stance>,
    /// A Dive is in flight.
    pub diving: bool,
}

/// The stance feedback the local Player last felt, for hearing what changed since.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct FeltStance(StanceFeel);

/// The gravity the Player falls and jumps under, after their `Stat::PlayerGravity` entries.
pub fn player_gravity(entries: &[Modifier]) -> f32 {
    resolve(Stat::PlayerGravity, GRAVITY, entries)
}

/// Plays the local Player's own stance sounds: a rustle at the start of every stance change, and
/// a thud when a Dive lands instead. Ignored stance input changes nothing, so it's silent.
pub(crate) fn stance_feedback(
    mut player: Single<(&Locomotion, &mut FeltStance, Has<LastStand>), With<LocalPlayer>>,
    mut cues: MessageWriter<PlayCue>,
) {
    let (locomotion, ref mut felt, last_stand) = *player;
    let now = locomotion.feel(last_stand);
    if let Some(cue) = stance_cue(felt.0, now) {
        cues.write(PlayCue::new(cue, None));
    }
    felt.0 = now;
}

/// How fast the eye drops in a Down's collapse, and climbs in the rise after: the standing eye to
/// the prone eye in `LAST_STAND_COLLAPSE_SECS`.
fn collapse_rate() -> f32 {
    (Stance::Standing.eye_height() - Stance::Prone.eye_height()) / LAST_STAND_COLLAPSE_SECS
}

/// A factor on the camera's FOV from an effect, like the Teleporter's FOV warp. 1 for none. Its
/// source rewrites it every frame the effect lasts and sets it back to 1 after.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct FovWarp(pub f32);

impl Default for FovWarp {
    fn default() -> Self {
        Self(1.0)
    }
}

/// A request to move `player` somewhere else at once (the Teleporter), putting their feet on
/// `feet`, turned to `facing` if given. Writing the Transform from `Update` doesn't work:
/// whenever the next frame runs a physics step, the interpolation first completes its last easing
/// and puts the Player straight back. The move is made at the start of the next physics step
/// instead, as a clean cut with no smear, and stops the Player.
#[derive(Message, Debug, Clone, Copy)]
pub struct Teleport {
    pub player: Entity,
    pub feet: Vec3,
    pub facing: Option<Quat>,
}

/// The Teleports waiting for the next physics step. A frame can pass with no step, and a message
/// would be gone by the next one.
#[derive(Resource, Default)]
struct PendingTeleports(Vec<Teleport>);

/// Holds each `Teleport` until a physics step carries it out.
fn queue_teleports(mut teleports: MessageReader<Teleport>, mut pending: ResMut<PendingTeleports>) {
    pending.0.extend(teleports.read().copied());
}

/// A Player took damage: a Zombie's swing, a blast, a Trap, a fire pool.
#[derive(Message)]
pub struct PlayerHit {
    /// The Player hit.
    pub player: Entity,
    pub damage: f32,
    /// Where the hit came from, for the damage indicator: the Zombie that swung, the blast's
    /// centre, the Trap. None for a hit with no direction to show, such as the Player's own
    /// explosive or a fire pool underfoot.
    pub from: Option<Vec3>,
}

/// Whether the local Player is clear to fire: not mid-prone-transition and not in a Dive's
/// landing recovery. Going down to Prone or getting up from it blocks firing — and the Knife
/// and Equipment, which count as firing — until the transition is done, and a Dive blocks from
/// its launch through `DIVE_RECOVERY_SECS` after landing; crouch transitions block nothing, and
/// no prone lock applies while Down — Last Stand's own rules don't change. (Aiming reads
/// `not_recovering_from_dive` for its share.)
pub fn not_changing_prone(players: Query<(&Locomotion, Has<LastStand>), With<LocalPlayer>>) -> bool {
    players.iter().all(|(locomotion, last_stand)| {
        // Nothing is fired, thrown or planted on a Zipline either.
        !locomotion.riding && (last_stand || (!locomotion.prone_transition() && !locomotion.recovering_from_dive()))
    })
}

/// The local Player is on a Zipline: the Weapon is lowered, so no fire, aim, Reload, throw or
/// Knife. Kept to `Locomotion::riding` by `mark_riding`.
#[derive(Component)]
pub struct Riding;

/// Marks the local Player `Riding` for as long as their ride lasts.
fn mark_riding(mut commands: Commands, player: Single<(Entity, &Locomotion, Has<Riding>), With<LocalPlayer>>) {
    let (entity, locomotion, marked) = *player;
    if locomotion.riding && !marked {
        commands.entity(entity).insert(Riding);
    } else if !locomotion.riding && marked {
        commands.entity(entity).remove::<Riding>();
    }
}

/// Places the camera's `Eye` from the glide: the eye's height above the feet, less the body's half
/// height. The Down's collapse and rise glide the same eye, toward the prone eye and back.
pub(crate) fn place_eye(player: Single<&Locomotion, With<LocalPlayer>>, mut eye: Single<&mut Eye, With<PlayerCamera>>) {
    eye.height = player.eye - player.shape().half_height();
}

/// Mouse look. It turns the Player's Transform every frame, so it runs right after the
/// interpolation has eased the Player's translation and before the interpolation notes that it
/// has done so. Turning the Player in `Update` instead counts as a teleport to the interpolation:
/// it throws away the easing, the Player freezes on frames without a physics step and the next
/// step starts from the half-eased spot, losing distance (the old "janky" movement).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct LookSystems;

fn configure_look(app: &mut App) {
    app.configure_sets(
        RunFixedMainLoop,
        LookSystems
            .in_set(RunFixedMainLoopSystems::AfterFixedMainLoop)
            .after(TransformEasingSystems::Ease)
            .before(TransformEasingSystems::UpdateEasingTick),
    );
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        configure_look(app);
        app.add_message::<PlayerHit>()
            .add_message::<HealPlayer>()
            .init_resource::<FovWarp>()
            .init_resource::<RunModifiers>()
            .init_resource::<StartingLoadout>()
            .add_systems(
                OnEnter(GameState::Playing),
                (spawn_player.in_set(SpawnPlayers), |mut warp: ResMut<FovWarp>| *warp = FovWarp::default()),
            )
            .add_systems(OnEnter(Pause::Running), grab_cursor)
            .add_systems(OnExit(Pause::Running), release_cursor)
            // Settings apply immediately, even from Pause.
            .add_systems(Update, apply_fov)
            .add_message::<Teleport>()
            .init_resource::<PendingTeleports>()
            .add_systems(PostUpdate, queue_teleports)
            .add_systems(
                FixedUpdate,
                (teleport_player, fill_move_input, move_player, stance_feedback).chain().run_if(world_runs),
            )
            .add_systems(RunFixedMainLoop, look.run_if(input_runs).in_set(LookSystems))
            .add_systems(
                Update,
                (take_hits, heal_players, regenerate).chain().in_set(HostSimulation).run_if(world_runs),
            )
            // The camera rides the stance glide, the Down's collapse and rise included; the sway
            // reads what it left.
            .add_systems(Update, (place_eye, mark_riding))
            // The OS can drop the grab (e.g. on focus loss); clicking takes it back.
            .add_systems(
                Update,
                grab_cursor
                    .run_if(input_runs)
                    .run_if(input_just_pressed(MouseButton::Left)),
            );
    }
}

/// A new Run's Points, counters and timers for a Player.
fn run_state(run: &[Modifier], map: MapId) -> impl Bundle {
    (
        Points::starting(run),
        RunStats::default(),
        PlayerZone::start(map),
        Shellshocked::default(),
        Cooldowns::default(),
        SparesUsed::default(),
    )
}

/// Where a Seat's Player starts: the Seat's spawn point on the Map, with the Player standing on it.
fn seat_spawn(seat: Seat, map: MapId) -> Transform {
    let half_height = PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0;
    Transform::from_translation(map.layout().seat_start(seat.0) + Vec3::Y * (half_height + 0.05))
}

/// Everything a Player is at the start of a Run on `map`, in `seat`, whoever drives them. What makes one the
/// `LocalPlayer` (the camera and view model) is added by the caller.
pub(crate) fn player_body(seat: Seat, name: &str, run: &[Modifier], map: MapId) -> impl Bundle {
    (
        InRun,
        Name::new(name.to_string()),
        (Player, seat, PlayerName(name.to_string())),
        Health {
            current: PLAYER_MAX_HEALTH,
            max: PLAYER_MAX_HEALTH,
            since_hit: HEALTH_REGEN_DELAY_SECS,
        },
        (
            Locomotion::default(),
            MoveInput::default(),
            StanceKeys::default(),
            FeltStance::default(),
            (Pose::default(), Shots::default(), HeldWeapon::default()),
        ),
        Visibility::default(),
        Loadout::new(STARTING_WEAPON),
        Modifiers(run.to_vec()),
        (Knife::default(), Equipment::default(), run_state(run, map), PlankTally::default()),
        (
            seat_spawn(seat, map),
            RigidBody::Kinematic,
            CustomPositionIntegration,
            TranslationInterpolation,
            Stance::Standing.collider(),
            CollisionLayers::new(GameLayer::Player, LayerMask::ALL),
        ),
    )
}

/// Spawns the Players a Run starts with. The Players are in the world once this has run (see
/// `RunType`).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SpawnPlayers;

pub(crate) fn spawn_player(
    mut commands: Commands,
    settings: Res<Settings>,
    run: Res<RunModifiers>,
    starting: Res<StartingLoadout>,
    party: Option<Res<Party>>,
    map: Res<MapId>,
) {
    let map = *map;
    // Solo is a party of one. In a Co-op Run every Seat has a Player, and only this machine's is
    // the `LocalPlayer`.
    let (seats, local) = match &party {
        Some(party) => (party.seats().to_vec(), party.local),
        None => (vec![(Seat(0), "Player".to_string())], Seat(0)),
    };
    for (seat, name) in seats {
        if seat != local {
            // The Host swaps in their own Kit's start once it knows it.
            commands.spawn((player_body(seat, &name, &[], map), StartingLoadout::default()));
            continue;
        }
        commands
            .spawn((player_body(seat, &name, &run.0, map), LocalPlayer))
            // Only this Player's own start, as with their Tracks.
            .insert((*starting, starting.loadout(), starting.knife(), starting.equipment()))
            .with_child((
                PlayerCamera,
                Eye {
                    height: PLAYER_EYE_HEIGHT,
                },
                Camera3d::default(),
                Projection::Perspective(PerspectiveProjection {
                    fov: settings.fov_radians(),
                    ..default()
                }),
                // Neutral, so effects (the Teleporter's black and white) can grade it.
                ColorGrading::default(),
                DistanceFog {
                    color: Color::srgb(0.02, 0.02, 0.04),
                    falloff: FogFalloff::Linear {
                        start: FOG_START,
                        end: FOG_END,
                    },
                    ..default()
                },
                Transform::from_xyz(0.0, PLAYER_EYE_HEIGHT, 0.0),
            ));
    }
}

fn grab_cursor(mut cursor: Single<&mut CursorOptions>, choosing: Query<(), (With<LocalPlayer>, With<Choosing>)>) {
    // An open Offer needs the pointer, whatever else just closed.
    if !choosing.is_empty() {
        return;
    }
    cursor.visible = false;
    cursor.grab_mode = CursorGrabMode::Locked;
}

fn release_cursor(mut cursor: Single<&mut CursorOptions>) {
    cursor.visible = true;
    cursor.grab_mode = CursorGrabMode::None;
}

/// The FOV setting, narrowed while aiming down the sights, and warped by any effect.
fn apply_fov(
    settings: Res<Settings>,
    warp: Res<FovWarp>,
    loadout: Single<&Loadout, With<LocalPlayer>>,
    mut cameras: Query<&mut Projection, With<PlayerCamera>>,
) {
    let fov = settings.fov_radians() * loadout.fov_factor() * warp.0;
    for mut projection in &mut cameras {
        if let Projection::Perspective(perspective) = &mut *projection
            && perspective.fov != fov
        {
            perspective.fov = fov;
        }
    }
}

fn look(
    mouse: Res<AccumulatedMouseMotion>,
    settings: Res<Settings>,
    player: Single<(&mut Transform, &Locomotion), With<LocalPlayer>>,
    mut camera: Single<&mut Transform, (With<PlayerCamera>, Without<LocalPlayer>)>,
) {
    let delta = settings.look_delta(mouse.delta);
    let (mut transform, locomotion) = player.into_inner();
    transform.rotate_y(-delta.x);

    const PITCH_LIMIT: f32 = FRAC_PI_2 - 0.01;
    // While Prone, or Down (which shares the prone camera), looking up is capped at
    // `PRONE_LOOK_UP_MAX` above level; looking down and turning stay free.
    let up = if locomotion.shape() == Stance::Prone { PRONE_LOOK_UP_MAX } else { PITCH_LIMIT };
    let (_, pitch, _) = camera.rotation.to_euler(EulerRot::YXZ);
    let pitch = (pitch - delta.y).clamp(-PITCH_LIMIT, up);
    camera.rotation = Quat::from_rotation_x(pitch);
}

/// Carries out a `Teleport` at the start of the physics step. The easing starts from the new
/// spot too, so no frame shows the Player partway there. A Teleporter ride puts the Player
/// Standing.
fn teleport_player(
    mut pending: ResMut<PendingTeleports>,
    mut players: Query<
        (
            &mut Transform,
            &mut Locomotion,
            &mut Collider,
            &mut TranslationEasingState,
            Option<&mut StanceKeys>,
        ),
        With<Player>,
    >,
) {
    for to in pending.0.drain(..) {
        let Ok((mut transform, mut locomotion, mut collider, mut easing, keys)) = players.get_mut(to.player) else {
            continue;
        };
        if let Some(mut keys) = keys {
            keys.stood_up();
        }
        transform.translation = to.feet + Vec3::Y * (Stance::Standing.half_height() + 0.05);
        if let Some(facing) = to.facing {
            transform.rotation = facing;
        }
        easing.start = Some(transform.translation);
        locomotion.velocity = Vec3::ZERO;
        locomotion.grounded = false;
        locomotion.stand();
        *collider = Stance::Standing.collider();
    }
}

/// Fills the local Player's `MoveInput` for the tick from the keyboard and the way they're
/// facing: the yaw is the Player's, the pitch the camera's, both as `look` has turned them. The
/// Crouch and Prone keys run through the Stance resolver, so the Stance wanted arrives already
/// resolved from the Toggle or Hold setting.
pub(crate) fn fill_move_input(
    controls: Controls,
    settings: Res<Settings>,
    player: Single<(&Transform, &mut MoveInput, &mut StanceKeys, &Locomotion, Has<LastStand>, Has<Noclip>), With<LocalPlayer>>,
    camera: Single<&Transform, (With<PlayerCamera>, Without<LocalPlayer>)>,
    zipline: Option<Res<ZiplineFocus>>,
) {
    let (transform, mut input, mut keys, locomotion, last_stand, noclip) = player.into_inner();
    let forward = controls.pressed(Action::Forward) as i8 - controls.pressed(Action::Back) as i8;
    let right = controls.pressed(Action::Right) as i8 - controls.pressed(Action::Left) as i8;
    let jump = keys.resolve(
        settings.stance_keys,
        controls.pressed(Action::Crouch),
        controls.pressed(Action::Prone),
        controls.pressed(Action::Jump),
        controls.pressed(Action::Sprint),
        locomotion.grounded(),
        last_stand,
    );
    let (yaw, _, _) = transform.rotation.to_euler(EulerRot::YXZ);
    let (_, pitch, _) = camera.rotation.to_euler(EulerRot::YXZ);
    *input = MoveInput {
        axes: Vec2::new(right as f32, forward as f32),
        jump,
        grab: controls.pressed(Action::Use) && zipline.is_some_and(|focus| focus.0),
        sprint: controls.pressed(Action::Sprint),
        aim: controls.pressed(Action::Aim),
        crouch: keys.target == Stance::Crouched,
        prone: keys.target == Stance::Prone,
        noclip,
        yaw,
        pitch,
    };
}

/// What `step_move` needs to know about a Player besides their input and `Locomotion`: who they
/// are and what they're carrying and doing. The body's shape comes from `Locomotion` itself: the
/// step works from the Stance it keeps, and the caller syncs the `Collider` component afterwards.
pub struct MoveContext<'a> {
    pub entity: Entity,
    pub loadout: &'a Loadout,
    pub modifiers: &'a Modifiers,
    pub drinking: bool,
    pub last_stand: bool,
    /// The Jump Pads that throw right now (see `jump_pad::working`).
    pub jump_pads: &'static [JumpPad],
    /// The Ziplines that work now (see `zipline::working`).
    pub ziplines: &'a [Zipline],
    /// The Map's pools, to wade through: `None` where there is no Map (a bare test world).
    pub layout: Option<&'static crate::maps::map::Layout>,
    /// The Tide level the Map's water stands at.
    pub tide: crate::maps::water::TideLevel,
}

/// Runs this tick's `MoveInput` for every Player but a client's puppets, which move as the Host
/// says. The step itself is `step_move`. A stance change shifts the body in one go, so its easing
/// is cut the way a `Teleport`'s is, the collider is put into the new shape, and no frame shows
/// the Player partway down or up.
pub(crate) fn move_player(
    mut commands: Commands,
    mut players: Query<
        (Entity, &mut Transform, &mut Locomotion, &MoveInput, &Loadout, &Modifiers, Has<Drinking>, Has<LastStand>),
        (With<Player>, Without<Puppet>),
    >,
    mut easings: Query<&mut TranslationEasingState>,
    move_and_slide: MoveAndSlide,
    time: Res<Time>,
    (map, power, open): (Option<Res<MapId>>, Option<Res<Power>>, Option<Res<OpenZones>>),
    tide: Option<Res<crate::maps::tide::Tide>>,
) {
    let tide = tide.map_or_else(Default::default, |tide| tide.level());
    let jump_pads = map.as_ref().map_or(&[][..], |map| crate::maps::jump_pad::working(**map, power.is_some_and(|power| power.is_on())));
    let ziplines = map.as_ref().map_or(Vec::new(), |map| crate::maps::zipline::working(**map, open.as_ref().map_or(Default::default(), |open| open.0)));
    let layout = map.map(|map| map.layout());
    for (entity, mut transform, mut locomotion, input, loadout, modifiers, drinking, last_stand) in &mut players {
        let mut context = MoveContext { entity, loadout, modifiers, drinking, last_stand, jump_pads, ziplines: &ziplines, layout, tide };
        if let Some(stance) = step_move(&mut transform.translation, &mut locomotion, input, &mut context, time.delta(), &move_and_slide) {
            commands.entity(entity).insert(stance.collider());
            if let Ok(mut easing) = easings.get_mut(entity) {
                easing.start = Some(transform.translation);
            }
        }
    }
}

/// One tick of flight: the body goes where the wish and Jump or Crouch say at a steady pace, through
/// walls, with no gravity and no stance change. Whatever else the body was doing is over.
fn fly(position: &mut Vec3, locomotion: &mut Locomotion, input: &MoveInput, rotation: Quat, dt: f32) {
    let wish = input.axes.normalize_or_zero();
    let up = f32::from(input.jump) - f32::from(input.crouch || input.prone);
    let direction = rotation * Vec3::new(wish.x, 0.0, -wish.y) + Vec3::Y * up;
    let speed = NOCLIP_SPEED * if input.sprint { NOCLIP_SPRINT_MULT } else { 1.0 };
    locomotion.velocity = direction.clamp_length_max(1.0) * speed;
    *position += locomotion.velocity * dt;
    locomotion.flying = true;
    locomotion.grounded = false;
    locomotion.sprinting = false;
    locomotion.diving = false;
    locomotion.thrown = false;
    locomotion.riding = false;
}

/// One tick of a Player's movement: advances `position` and `Locomotion` by `delta` under
/// `input`. It reads no keyboard and no clock, so given the same start, input and world it
/// reaches the same place every time, which is what lets a client replay inputs through it.
/// Returns the shape the body took this tick (see `Locomotion::shape`), if it changed.
pub fn step_move(
    position: &mut Vec3,
    locomotion: &mut Locomotion,
    input: &MoveInput,
    context: &mut MoveContext,
    delta: Duration,
    move_and_slide: &MoveAndSlide,
) -> Option<Stance> {
    let MoveContext { entity, loadout, modifiers, drinking, last_stand, jump_pads, ziplines, layout, tide } = *context;
    let dt = delta.as_secs_f32();
    let rotation = Quat::from_rotation_y(input.yaw);

    let wish = input.axes.normalize_or_zero();

    // Wading: feet in Shallow Water slow every step, and there's no lying down in it (the camera
    // would be under the surface). It's worked out here from where the feet are, as the Jump Pads
    // are, so a client replaying its inputs wades exactly where the Host does.
    let wading = layout.is_some_and(|layout| crate::maps::water::wading(layout, tide, *position - Vec3::Y * locomotion.shape().half_height()));

    let mut changed = None;
    // The Down pose: a Down from any Stance, on the ground or in the air, lays the body down at
    // once, feet staying put; the end of the Down (a Revive or Self-Revive) rises it to Standing.
    // There's no route back to Crouched or Prone through a Down.
    if last_stand != locomotion.down {
        position.y += if last_stand { locomotion.collapse() } else { locomotion.rise() };
        changed = Some(locomotion.shape());
    }

    // A Zipline: grabbed by a Player who is up, on the ground, Standing or Crouched and not in the
    // middle of anything else, and then ridden to the end, whatever else the input says.
    if !locomotion.riding && input.grab && can_grab(locomotion, last_stand, drinking) {
        let feet = *position - Vec3::Y * locomotion.shape().half_height();
        if let Some(line) = crate::maps::zipline::grabbable(ziplines, feet) {
            // A Crouched Player stands up as they grab: the handle is overhead.
            locomotion.stand();
            locomotion.riding = true;
            locomotion.grounded = false;
            locomotion.sprinting = false;
            locomotion.velocity = line.direction() * ZIPLINE_START_SPEED;
            *position = line.from + Vec3::Y * Stance::Standing.half_height();
            changed = Some(Stance::Standing);
        }
    }
    if input.noclip {
        fly(position, locomotion, input, rotation, dt);
        return changed;
    }
    if locomotion.flying {
        // Down from a flight: nothing of its speed is carried, and the body is back on the Map.
        locomotion.flying = false;
        locomotion.left_flight = true;
        locomotion.velocity = Vec3::ZERO;
    }
    if locomotion.riding {
        ride(position, locomotion, ziplines, dt);
        return changed;
    }

    // Stance: the input carries the Stance wanted, resolved from the keys. The body takes it at
    // the start of a transition, both ways: going down shrinks the capsule at once, feet staying
    // put; getting up passes the ceiling check first, then grows. Nothing changes in the air.
    let target = if last_stand {
        Stance::Standing
    } else if input.prone && wading {
        // No going Prone or Diving in Shallow Water, and a Prone Player crawling in rises to Crouched.
        Stance::Crouched
    } else if input.prone {
        Stance::Prone
    } else if input.crouch {
        Stance::Crouched
    } else {
        Stance::Standing
    };
    // The Dive: the target going Prone while sprinting is a lunge into a low airborne arc, not a
    // prone transition — detected here from the same bits the Host replays, so both sides see it
    // the same. It starts only from the ground (the sprint is grounded), never under the landing
    // recovery, and only once the last Dive's cooldown has passed. On the cooldown the press is
    // ignored outright — no Dive and no stance change either; the sprint carries on.
    let diving = locomotion.sprinting && target == Stance::Prone && locomotion.stance == Stance::Standing;
    if locomotion.stance != target
        && locomotion.grounded
        && !locomotion.recovering_from_dive()
        && (diving && !locomotion.dive_on_cooldown()
            || !diving
                && (target.depth() > locomotion.stance.depth()
                    || room_for(move_and_slide, entity, *position, locomotion.stance, target)))
    {
        position.y += target.half_height() - locomotion.stance.half_height();
        locomotion.left_stance = locomotion.stance;
        locomotion.stance = target;
        locomotion.stance_change_secs = transition_secs(locomotion.left_stance, target);
        locomotion.since_stance_change = 0.0;
        // The camera keeps gliding between the eyes for the whole transition.
        locomotion.eye_rate = (target.eye_height() - locomotion.eye).abs() / locomotion.stance_change_secs;
        if diving {
            // The lunge leaves the floor at once, under gravity: `DIVE_DISTANCE` over about
            // `DIVE_FLIGHT_SECS`, ending Prone where it lands through the transition the lines
            // above began. Off a ledge the same velocity just keeps falling.
            locomotion.diving = true;
            locomotion.grounded = false;
            let lunge = rotation * Vec3::NEG_Z * (DIVE_DISTANCE / DIVE_FLIGHT_SECS);
            locomotion.velocity = lunge + Vec3::Y * (GRAVITY * DIVE_FLIGHT_SECS / 2.0);
        }
        changed = Some(target);
    }
    // The step works from the body's own shape; the caller puts the collider component into it.
    let shape = locomotion.shape().collider();
    locomotion.since_stance_change += dt;
    // (The Dive's timers count from its landing, which is found further down, once the move has
    // run. `INFINITY` stays itself.)
    locomotion.since_dive_landing += dt;
    // The camera keeps gliding to the body's eye for the whole transition.
    locomotion.eye = move_towards(locomotion.eye, locomotion.shape().eye_height(), locomotion.eye_rate * dt);

    // Sprinting only makes sense running forwards on the ground, and it runs out. Aiming stops
    // it, there's no sprinting with a Perk in hand or in Last Stand, and only Standing sprints:
    // from Crouched, Sprint stands and sprints at once; from Prone, it stands the Player up and
    // the sprint starts once upright if Sprint is still held.
    let aiming = input.aim && loadout.can_aim();
    let rising_from_prone = locomotion.changing_stance() && locomotion.left_stance == Stance::Prone;
    let wants_sprint = input.sprint
        && input.axes.y > 0.0
        && locomotion.grounded
        && !aiming
        && !drinking
        && !last_stand
        && locomotion.stance == Stance::Standing
        && !rising_from_prone;
    locomotion.sprinting = wants_sprint && !locomotion.winded;
    if locomotion.sprinting {
        locomotion.stamina -= dt * resolve(Stat::SprintDrain, 1.0, &modifiers.0);
        if locomotion.stamina <= 0.0 {
            locomotion.stamina = 0.0;
            locomotion.winded = true;
        }
    } else {
        locomotion.stamina += dt * SPRINT_DURATION_SECS / SPRINT_RECHARGE_SECS;
        if locomotion.stamina >= SPRINT_DURATION_SECS {
            locomotion.stamina = SPRINT_DURATION_SECS;
            locomotion.winded = false;
        }
    }
    // Mid-transition the Player moves at the slower Stance's speed: the one just left.
    let moving_as = if locomotion.changing_stance()
        && locomotion.left_stance.speed_mult() < locomotion.stance.speed_mult()
    {
        locomotion.left_stance
    } else {
        locomotion.stance
    };
    let base = if last_stand {
        PLAYER_WALK_SPEED * LAST_STAND_CRAWL_MULT
    } else if locomotion.sprinting {
        PLAYER_SPRINT_SPEED
    } else {
        PLAYER_WALK_SPEED * moving_as.speed_mult()
    };
    let speed = loadout.move_speed(base, &modifiers.0) * if wading { WADE_SPEED_MULT } else { 1.0 };

    let wish_world = rotation * Vec3::new(wish.x, 0.0, -wish.y) * speed;
    let horizontal = Vec3::new(locomotion.velocity.x, 0.0, locomotion.velocity.z);
    // In flight the Dive steers only slightly: the held direction turns the lunge a little and
    // nothing slows it. A Jump Pad's throw isn't steered at all, so it lands where it aims.
    // Anywhere else in the air the wish moves the Player as always.
    let horizontal = if locomotion.thrown {
        horizontal
    } else if locomotion.diving {
        let steer = rotation * Vec3::new(wish.x, 0.0, -wish.y);
        if steer.length_squared() > 0.0 {
            horizontal.move_towards(steer * horizontal.length(), DIVE_STEER_ACCEL * dt)
        } else {
            horizontal
        }
    } else {
        horizontal.move_towards(wish_world, PLAYER_ACCELERATION * dt)
    };
    // Gravity only pulls in the air; on the floor it would just push the Player into it, and
    // slowly down the ramp while standing still.
    // A Jump Pad's throw is a fixed ballistic arc, so it falls under the base gravity whatever the
    // stat says.
    let gravity = if locomotion.thrown { GRAVITY } else { player_gravity(&modifiers.0) };
    let mut vertical = if locomotion.grounded { 0.0 } else { locomotion.velocity.y - gravity * dt };
    // No jumping from a crawl, and no jumping out of a Stance: Jump while Crouched or Prone
    // stands the Player up instead (the resolver turned the press into the stand-up).
    let jumped = locomotion.grounded && input.jump && !last_stand && locomotion.stance == Stance::Standing;
    if jumped {
        vertical = PLAYER_JUMP_SPEED;
    }
    let mut velocity = horizontal + Vec3::Y * vertical;
    // A Player on the floor standing on a working Jump Pad is thrown at its landing spot, whatever
    // they're doing but crawling in a Last Stand.
    let feet = *position - Vec3::Y * locomotion.shape().half_height();
    let thrown = (locomotion.grounded && !last_stand)
        .then(|| jump_pads.iter().find(|pad| pad.under(feet)))
        .flatten()
        .map(|pad| crate::maps::jump_pad::throw(feet, pad.to));
    if let Some(throw) = thrown {
        velocity = throw;
        locomotion.thrown = true;
        locomotion.sprinting = false;
    }
    let jumped = jumped || thrown.is_some();

    let config = MoveAndSlideConfig::default();
    let filter = SpatialQueryFilter::from_mask([GameLayer::World, GameLayer::Zombie, GameLayer::Window, GameLayer::WaterEdge])
        .with_excluded_entities([entity]);
    let mut grounded = false;
    let out = move_and_slide.move_and_slide(
        &shape,
        *position,
        rotation,
        velocity,
        delta,
        &config,
        &filter,
        |hit| {
            if hit.normal.y > PLAYER_GROUND_NORMAL_Y {
                grounded = true;
            }
            MoveAndSlideHitResponse::Accept
        },
    );
    let mut new_position = out.position;
    let mut velocity = out.projected_velocity;

    // Move-and-slide only touches the floor on steps where gravity's pull reaches it through the
    // skin, so on its own `grounded` flickered every other step on flat floor (and sprint with
    // it), and the ramp's slope carried the Player up off it. A short probe down finds the floor
    // every step and keeps the Player on it.
    if !grounded
        && probes_for_ground(locomotion.grounded, jumped, vertical)
        && let Some(hit) = move_and_slide.cast_move(
            &shape,
            new_position,
            rotation,
            Vec3::NEG_Y * PLAYER_GROUND_PROBE,
            config.skin_width,
            &filter,
        )
        && hit.normal1.y > PLAYER_GROUND_NORMAL_Y
    {
        new_position.y -= hit.distance;
        grounded = true;
    }
    // A jump leaves the floor even if it's still within reach this step.
    let grounded = grounded && !jumped;
    // On the floor the Player's speed carries on along the ground, so falling stops and walking up
    // the ramp doesn't launch them off the top of it.
    if grounded {
        velocity = velocity.with_y(0.0).normalize_or_zero() * velocity.length();
    }
    // A Dive lands where it is: back on the floor after the lunge. Recovery and the cooldown both
    // run from here, and the stance it lands in is the one it left in — Prone.
    if locomotion.diving && grounded {
        locomotion.diving = false;
        locomotion.since_dive_landing = 0.0;
    }
    // A throw ends where it comes down, feet planted, with none of its speed carried on.
    if locomotion.thrown && grounded {
        locomotion.thrown = false;
        velocity = Vec3::ZERO;
    }

    *position = new_position;
    locomotion.velocity = velocity;
    locomotion.grounded = grounded;
    changed
}

/// Whether a Player can grab a Zipline's handle: up, on the ground, Standing or Crouched, and
/// not Diving, thrown, recovering from a Dive or drinking a Perk.
fn can_grab(locomotion: &Locomotion, last_stand: bool, drinking: bool) -> bool {
    !last_stand
        && !locomotion.down
        && !drinking
        && locomotion.grounded
        && !locomotion.diving
        && !locomotion.thrown
        && !locomotion.recovering_from_dive()
        && locomotion.stance != Stance::Prone
        && locomotion.left_stance != Stance::Prone
}

/// One tick of a Zipline ride: the body is moved kinematically along the line, passing through
/// anything on it, and nothing in the input matters. The line is the one the feet are on. At the
/// end the feet are put at the landing and the ride is over, with walk-speed momentum along the
/// line's heading; the next step puts them on the floor.
fn ride(position: &mut Vec3, locomotion: &mut Locomotion, ziplines: &[Zipline], dt: f32) {
    let half = locomotion.shape().half_height();
    let feet = *position - Vec3::Y * half;
    let Some(line) = crate::maps::zipline::ridden(ziplines, feet) else {
        // No line under the Player (a closed Map's, a bare world): the ride is over where they are.
        locomotion.riding = false;
        return;
    };
    locomotion.ride_to = line.to;
    locomotion.since_stance_change += dt;
    locomotion.since_dive_landing += dt;
    locomotion.eye = move_towards(locomotion.eye, locomotion.shape().eye_height(), locomotion.eye_rate * dt);
    // Stamina recharges on the ride.
    locomotion.sprinting = false;
    locomotion.stamina += dt * SPRINT_DURATION_SECS / SPRINT_RECHARGE_SECS;
    if locomotion.stamina >= SPRINT_DURATION_SECS {
        locomotion.stamina = SPRINT_DURATION_SECS;
        locomotion.winded = false;
    }
    let speed = locomotion.velocity.length().max(ZIPLINE_START_SPEED);
    let (feet, speed, arrived) = crate::maps::zipline::advance(line, line.at(line.along(feet).1), speed, dt);
    *position = feet + Vec3::Y * half;
    locomotion.grounded = false;
    if arrived {
        locomotion.riding = false;
        locomotion.velocity = line.heading() * PLAYER_WALK_SPEED + Vec3::Y * line.direction().y * speed;
    } else {
        locomotion.velocity = line.direction() * speed;
    }
}

/// Seconds one Stance change takes, by the pair it joins: Standing↔Crouched
/// `STANCE_CROUCH_SECS`, Crouched↔Prone `STANCE_PRONE_SECS`, Standing↔Prone
/// `STANCE_PRONE_FROM_STAND_SECS` (through Crouched).
pub(crate) fn transition_secs(from: Stance, to: Stance) -> f32 {
    match (from, to) {
        (Stance::Standing, Stance::Crouched) | (Stance::Crouched, Stance::Standing) => STANCE_CROUCH_SECS,
        (Stance::Crouched, Stance::Prone) | (Stance::Prone, Stance::Crouched) => STANCE_PRONE_SECS,
        _ => STANCE_PRONE_FROM_STAND_SECS,
    }
}

/// Whether the `to` Stance's body fits where the Player is, feet staying put: the ceiling check
/// that can refuse a stand-up — or a rise out of Prone into Crouched. The floor sits a skin's
/// width below the feet, so it never counts.
fn room_for(move_and_slide: &MoveAndSlide, entity: Entity, position: Vec3, from: Stance, to: Stance) -> bool {
    let risen = position + Vec3::Y * (to.half_height() - from.half_height());
    let filter = SpatialQueryFilter::from_mask([GameLayer::World, GameLayer::Zombie, GameLayer::Window])
        .with_excluded_entities([entity]);
    let mut room = true;
    move_and_slide.intersections(
        &to.collider(),
        risen,
        Quat::IDENTITY,
        0.0,
        &filter,
        |_, _, _| {
            room = false;
            false
        },
    );
    room
}

/// Whether a step looks down for floor to stay on: always while on the floor, unless jumping off
/// it; in the air, only once falling, to land.
fn probes_for_ground(was_grounded: bool, jumped: bool, vertical_speed: f32) -> bool {
    if jumped {
        false
    } else {
        was_grounded || vertical_speed <= 0.0
    }
}

/// Hits take Health. Nothing hurts a Player in Last Stand. Reaching zero is a Down, which the
/// Last Stand module handles.
pub fn take_hits(
    mut hits: MessageReader<PlayerHit>,
    mut players: Query<(&mut Health, Option<&mut DownGuard>, Has<LastStand>, Has<AwaitingRespawn>, Has<Invincible>), With<Player>>,
) {
    for hit in hits.read() {
        let Ok((mut health, guard, last_stand, awaiting, invincible)) = players.get_mut(hit.player) else {
            continue;
        };
        if last_stand || awaiting || invincible {
            continue;
        }
        let charges = guard.as_ref().map_or(0, |guard| guard.charges);
        let (left, guarded) = guarded_hit(health.current, hit.damage, charges);
        if guarded && let Some(mut guard) = guard {
            guard.charges -= 1;
        }
        health.current = left;
        health.since_hit = 0.0;
    }
}

/// The Player can't be hurt or Downed: `take_hits` lets every hit pass them by, and being Out of
/// Bounds only puts them back (see `crate::players::out_of_bounds`). One in Last Stand gets up.
/// Classic never adds one; anything layered on it may (ADR 0007). It is its own marker, not a
/// `DownGuard`, which a layer rewrites.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Invincible;

/// Charges that each turn a hit that would Down the Player into one that leaves them on 1 Health.
/// Classic never adds one; anything layered on it may (ADR 0007).
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DownGuard {
    pub charges: u32,
}

/// Health left after a hit of `damage` on `current`, and whether a Down guard charge was spent on
/// it: with a charge, a hit that would Down leaves 1 Health instead.
pub fn guarded_hit(current: f32, damage: f32, charges: u32) -> (f32, bool) {
    let left = current - damage;
    if left <= 0.0 && charges > 0 {
        (1.0, true)
    } else {
        (left, false)
    }
}

/// Request to restore a Player's Health by `amount`, up to their max.
#[derive(Message, Debug, Clone, Copy)]
pub struct HealPlayer {
    pub player: Entity,
    pub amount: f32,
}

/// Heals asked for land, capped at max Health. A Player in Last Stand isn't healed.
pub fn heal_players(
    mut heals: MessageReader<HealPlayer>,
    mut players: Query<&mut Health, (With<Player>, Without<LastStand>)>,
) {
    for heal in heals.read() {
        if let Ok(mut health) = players.get_mut(heal.player) {
            health.current = (health.current + heal.amount.max(0.0)).min(health.max);
        }
    }
}

/// Health stays empty through a Last Stand; the Self-Revive refills it.
fn regenerate(
    mut players: Query<(&mut Health, &Modifiers), (With<Player>, Without<LastStand>)>,
    round: Option<Res<Round>>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    let in_round = round.is_some_and(|round| round.phase == RoundPhase::Active);
    for (mut health, modifiers) in &mut players {
        health.set_max(resolve(Stat::MaxHealth, PLAYER_MAX_HEALTH, &modifiers.0));
        health.since_hit += dt;
        if health.since_hit >= resolve(Stat::HealthRegenDelay, HEALTH_REGEN_DELAY_SECS, &modifiers.0) {
            let rate = regen_rate(health.max, in_round, &modifiers.0);
            health.current = (health.current + rate * dt).min(health.max);
        }
    }
}

/// Health regenerated per second once the regen delay is up: a full bar in
/// `HEALTH_REGEN_FULL_SECS`, slowed while a Round is on by the Player's `Stat::RoundRegen` entries.
pub fn regen_rate(max: f32, in_round: bool, player: &[Modifier]) -> f32 {
    let rate = max / HEALTH_REGEN_FULL_SECS;
    if in_round { resolve(Stat::RoundRegen, rate, player).max(0.0) } else { rate }
}

impl Health {
    /// Max Health is re-resolved every frame. Losing max Health (say, Juggernog on a Down) takes
    /// any Health above the new max with it; gaining it leaves the regen to fill the rest.
    pub fn set_max(&mut self, max: f32) {
        self.max = max;
        self.current = self.current.min(max);
    }

    pub fn fraction(&self) -> f32 {
        (self.current / self.max).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod headless;

#[cfg(test)]
pub(crate) mod two_players;

#[cfg(test)]
mod tests {
    use super::*;

    fn health(current: f32, max: f32) -> Health {
        Health {
            current,
            max,
            since_hit: 0.0,
        }
    }

    #[test]
    fn a_down_guard_leaves_one_health_and_spends_a_charge() {
        assert_eq!(guarded_hit(30.0, 50.0, 1), (1.0, true));
        assert_eq!(guarded_hit(30.0, 50.0, 0), (-20.0, false), "no charge, so it Downs");
        assert_eq!(guarded_hit(80.0, 50.0, 1), (30.0, false), "a hit that doesn't Down spends none");
        assert_eq!(guarded_hit(1.0, 50.0, 2), (1.0, true));
    }

    #[test]
    fn a_heal_restores_health_up_to_the_max() {
        let mut app = App::new();
        app.add_message::<HealPlayer>().add_systems(Update, heal_players);
        let player = app.world_mut().spawn((Player, health(50.0, 100.0))).id();
        app.world_mut().write_message(HealPlayer { player, amount: 10.0 });
        app.update();
        assert_eq!(app.world().get::<Health>(player).unwrap().current, 60.0);
        app.world_mut().write_message(HealPlayer { player, amount: 500.0 });
        app.update();
        assert_eq!(app.world().get::<Health>(player).unwrap().current, 100.0);
    }

    #[test]
    fn a_down_guard_turns_a_downing_hit_into_one_health() {
        let mut app = App::new();
        app.add_message::<PlayerHit>().add_systems(Update, take_hits);
        let player = app.world_mut().spawn((Player, health(30.0, 100.0), DownGuard { charges: 1 })).id();
        let hit = || PlayerHit { player, damage: 50.0, from: None };
        app.world_mut().write_message(hit());
        app.update();
        assert_eq!(app.world().get::<Health>(player).unwrap().current, 1.0);
        assert_eq!(app.world().get::<DownGuard>(player).unwrap().charges, 0);
        app.world_mut().write_message(hit());
        app.update();
        assert!(app.world().get::<Health>(player).unwrap().current <= 0.0, "the next one Downs");
    }

    #[test]
    fn an_invincible_player_takes_no_hit_and_a_down_guard_is_not_what_protects_them() {
        let mut app = App::new();
        app.add_message::<PlayerHit>().add_systems(Update, take_hits);
        let player = app.world_mut().spawn((Player, health(30.0, 100.0), Invincible)).id();
        app.world_mut().write_message(PlayerHit { player, damage: 500.0, from: None });
        app.update();
        assert_eq!(app.world().get::<Health>(player).unwrap().current, 30.0);
        // A layer rewriting the guard leaves the marker alone.
        app.world_mut().entity_mut(player).insert(DownGuard { charges: 0 });
        app.world_mut().write_message(PlayerHit { player, damage: 500.0, from: None });
        app.update();
        assert_eq!(app.world().get::<Health>(player).unwrap().current, 30.0);
    }

    #[test]
    fn toggle_flips_each_key_between_its_pair_of_stances() {
        use crate::storage::settings::StanceMode::*;
        let mut keys = StanceKeys::default();
        assert_eq!(keys.target, Stance::Standing);
        // A Crouch press flips Standing↔Crouched; letting go changes nothing.
        keys.resolve(Toggle, true, false, false, false, true, false);
        assert_eq!(keys.target, Stance::Crouched);
        keys.resolve(Toggle, false, false, false, false, true, false);
        assert_eq!(keys.target, Stance::Crouched);
        keys.resolve(Toggle, true, false, false, false, true, false);
        assert_eq!(keys.target, Stance::Standing);

        // A Prone press goes down from Standing, toggles back up from Prone, and from Crouched
        // goes down too. From Prone, Crouch goes to Crouched.
        keys.resolve(Toggle, false, true, false, false, true, false);
        assert_eq!(keys.target, Stance::Prone);
        keys.resolve(Toggle, false, false, false, false, true, false);
        assert_eq!(keys.target, Stance::Prone);
        keys.resolve(Toggle, false, true, false, false, true, false);
        assert_eq!(keys.target, Stance::Standing, "Prone didn't toggle back up");
        keys.resolve(Toggle, true, false, false, false, true, false);
        assert_eq!(keys.target, Stance::Crouched);
        keys.resolve(Toggle, false, true, false, false, true, false);
        assert_eq!(keys.target, Stance::Prone, "Prone didn't go down from Crouched");
        keys.resolve(Toggle, true, false, false, false, true, false);
        assert_eq!(keys.target, Stance::Crouched, "Crouch from Prone didn't land Crouched");
    }

    #[test]
    fn toggle_jump_and_sprint_stand_you_up_from_either_stance() {
        use crate::storage::settings::StanceMode::*;
        let mut keys = StanceKeys::default();
        keys.resolve(Toggle, false, true, false, false, true, false);
        assert_eq!(keys.target, Stance::Prone);
        // Jump stands the Player up instead of jumping, and doesn't jump while held. Once the
        // jump is let go, the next press jumps again.
        assert!(!keys.resolve(Toggle, false, false, true, false, true, false), "the stand-up jump jumped");
        assert_eq!(keys.target, Stance::Standing);
        assert!(!keys.resolve(Toggle, false, false, true, false, true, false), "the held jump jumped");
        keys.resolve(Toggle, false, false, false, false, true, false);
        assert!(keys.resolve(Toggle, false, false, true, false, true, false));

        // Sprint stands the Player up too; holding it doesn't stop a later Prone press.
        keys.resolve(Toggle, false, true, false, false, true, false);
        keys.resolve(Toggle, false, false, false, true, true, false);
        assert_eq!(keys.target, Stance::Standing);
        keys.resolve(Toggle, false, true, false, true, true, false);
        assert_eq!(keys.target, Stance::Prone, "the held sprint kept standing");
    }

    #[test]
    fn hold_keeps_you_in_the_held_stance_and_letting_go_of_prone_lands_you_crouched() {
        use crate::storage::settings::StanceMode::*;
        let mut keys = StanceKeys::default();
        // A held key keeps the Player in its Stance; letting go brings them up. Both held, Prone
        // wins; letting go of Prone while still holding Crouch lands them Crouched.
        keys.resolve(Hold, true, false, false, false, true, false);
        assert_eq!(keys.target, Stance::Crouched);
        keys.resolve(Hold, true, true, false, false, true, false);
        assert_eq!(keys.target, Stance::Prone, "both keys held didn't go Prone");
        keys.resolve(Hold, true, false, false, false, true, false);
        assert_eq!(keys.target, Stance::Crouched, "releasing Prone didn't land Crouched");
        keys.resolve(Hold, false, false, false, false, true, false);
        assert_eq!(keys.target, Stance::Standing);
        keys.resolve(Hold, false, true, false, false, true, false);
        assert_eq!(keys.target, Stance::Prone);
        keys.resolve(Hold, false, false, false, false, true, false);
        assert_eq!(keys.target, Stance::Standing);

        // Jump stands the Player up, and the keys give way until they are released and pressed
        // again.
        keys.resolve(Hold, true, true, false, false, true, false);
        assert!(!keys.resolve(Hold, true, true, true, false, true, false), "the stand-up jump jumped");
        assert_eq!(keys.target, Stance::Standing);
        keys.resolve(Hold, true, true, false, false, true, false);
        assert_eq!(keys.target, Stance::Standing, "the held keys went back down");
        keys.resolve(Hold, false, false, false, false, true, false);
        keys.resolve(Hold, false, true, false, false, true, false);
        assert_eq!(keys.target, Stance::Prone, "released and pressed again didn't go down");

        // Sprint wins over the held keys while it is held, and gives them back when let go.
        keys.resolve(Hold, true, true, false, true, true, false);
        assert_eq!(keys.target, Stance::Standing, "the held keys beat the sprint");
        keys.resolve(Hold, true, true, false, false, true, false);
        assert_eq!(keys.target, Stance::Prone, "the sprint's release didn't hand the keys back");
    }

    #[test]
    fn airborne_presses_are_ignored_and_a_down_resets_the_target() {
        use crate::storage::settings::StanceMode::*;
        for mode in [Toggle, Hold] {
            let mut keys = StanceKeys::default();
            // In the air nothing flips; a held key still lands them in its Stance when they do.
            keys.resolve(mode, true, true, false, false, false, false);
            assert_eq!(keys.target, if mode == Hold { Stance::Prone } else { Stance::Standing }, "{mode:?}");
            let mut down = StanceKeys { target: Stance::Prone, ..default() };
            // Stance input is dead while Down: the target comes back Standing.
            assert!(down.resolve(mode, true, true, true, true, true, true), "the Down took the jump");
            assert_eq!(down.target, Stance::Standing);
            // Up again, the keys held through the Down keep them Standing until pressed afresh.
            down.resolve(mode, true, true, false, false, true, false);
            assert_eq!(down.target, Stance::Standing, "{mode:?}: a key held through the Down went back down");
            down.resolve(mode, false, false, false, false, true, false);
            down.resolve(mode, false, true, false, false, true, false);
            assert_eq!(down.target, Stance::Prone, "{mode:?}: a fresh press after the get-up didn't go down");
        }
    }

    #[test]
    fn the_floor_is_looked_for_except_while_rising_through_the_air() {
        // Walking, including up the ramp, where the slope lifts the Player.
        assert!(probes_for_ground(true, false, 0.0));
        assert!(probes_for_ground(true, false, 3.0));
        // Jumping off the floor.
        assert!(!probes_for_ground(true, true, PLAYER_JUMP_SPEED));
        // Rising from a jump, then falling back.
        assert!(!probes_for_ground(false, false, 2.0));
        assert!(probes_for_ground(false, false, -2.0));
    }

    #[test]
    fn the_solo_player_is_seat_0_and_local_with_the_camera_on_it() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        world.insert_resource(Settings::default());
        world.init_resource::<RunModifiers>();
        world.init_resource::<StartingLoadout>();
        world.init_resource::<MapId>();
        world.run_system_once(spawn_player).unwrap();
        let (player, seat) = world
            .query_filtered::<(Entity, &Seat), (With<Player>, With<LocalPlayer>)>()
            .single(&world)
            .unwrap();
        assert_eq!(*seat, Seat(0));
        let camera = world.query_filtered::<&ChildOf, With<PlayerCamera>>().single(&world).unwrap();
        assert_eq!(camera.parent(), player);
    }

    #[test]
    fn the_starting_loadout_sets_the_local_players_opening_gear_and_defaults_to_classics() {
        use bevy::ecs::system::RunSystemOnce;
        use crate::weapons::arsenal::WeaponId;
        let spawned = |start: StartingLoadout| {
            let mut world = World::new();
            world.insert_resource(Settings::default());
            world.init_resource::<RunModifiers>();
            world.insert_resource(start);
            world.init_resource::<MapId>();
            world.run_system_once(spawn_player).unwrap();
            let (loadout, knife, equipment) = world
                .query_filtered::<(&Loadout, &Knife, &Equipment), With<LocalPlayer>>()
                .single(&world)
                .unwrap();
            (loadout.held().id, loadout.held().reserve, knife.bowie, equipment.frags)
        };
        let usual = WeaponId::M1911.stats().starting_reserve();
        assert_eq!(spawned(StartingLoadout::default()), (STARTING_WEAPON, usual, false, FRAG_START));
        let start = StartingLoadout { weapon: WeaponId::M14, reserve: Some(8), bowie: true, lethals: 4 };
        assert_eq!(spawned(start), (WeaponId::M14, 8, true, 4));
    }

    #[test]
    fn each_seat_has_its_own_colour_starting_with_white() {
        assert_eq!(Seat(0).colour(), Color::srgb(0.95, 0.95, 0.95));
        for a in 0..4u8 {
            for b in (a + 1)..4 {
                assert_ne!(Seat(a).colour(), Seat(b).colour());
            }
        }
    }

    #[test]
    fn raising_max_health_leaves_current_health_to_regen() {
        let mut h = health(100.0, 100.0);
        h.set_max(160.0);
        assert_eq!((h.current, h.max), (100.0, 160.0));
    }

    #[test]
    fn a_round_regen_entry_holds_health_back_until_the_intermission() {
        use crate::modifier::{Effect, Source};
        let none = [Modifier { stat: Stat::RoundRegen, source: Source("A"), effect: Effect::Mult(0.0) }];
        let full = PLAYER_MAX_HEALTH / HEALTH_REGEN_FULL_SECS;
        assert_eq!(regen_rate(PLAYER_MAX_HEALTH, true, &[]), full);
        assert_eq!(regen_rate(PLAYER_MAX_HEALTH, true, &none), 0.0);
        assert_eq!(regen_rate(PLAYER_MAX_HEALTH, false, &none), full);
    }

    #[test]
    fn lowering_max_health_caps_current_health() {
        let mut h = health(150.0, 160.0);
        h.set_max(100.0);
        assert_eq!((h.current, h.max), (100.0, 100.0));
    }
}
