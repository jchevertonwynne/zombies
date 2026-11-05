use std::f32::consts::FRAC_PI_2;
use std::time::Duration;

use avian3d::prelude::*;
use bevy::input::common_conditions::input_just_pressed;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::render::view::ColorGrading;
use bevy::window::{CursorGrabMode, CursorOptions};
use bevy_transform_interpolation::TranslationEasingState;

use crate::authority::HostSimulation;
use crate::barrier::PlankTally;
use crate::controls::{Action, Controls};
use crate::equipment::Equipment;
use crate::last_stand::{LastStand, lower_eye};
use crate::respawn::AwaitingRespawn;
use crate::layers::GameLayer;
use crate::map::seat_start;
use crate::party::Party;
use crate::net::play::Puppet;
use crate::modifier::{Modifier, Modifiers, RunModifiers, Stat, resolve};
use crate::perk::Drinking;
use crate::points::Points;
use crate::ammo_mod::Cooldowns;
use crate::last_stand::SparesUsed;
use crate::shellshock::Shellshocked;
use crate::stats::RunStats;
use crate::zone::PlayerZone;
use crate::round::{Round, RoundPhase};
use crate::settings::{Settings, StanceMode};
use crate::state::{Choosing, GameState, InRun, Pause, input_runs, world_runs};
use crate::teammate::{HeldWeapon, Pose, Shots};
use crate::tuning::*;
use crate::weapon::{Knife, Loadout, move_towards};

/// How low a Player's body is. Standing today, with Crouched behind the Crouch key; Prone comes
/// with its own slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Stance {
    #[default]
    Standing,
    Crouched,
}

impl Stance {
    /// The other of the two.
    pub fn other(self) -> Self {
        match self {
            Stance::Standing => Stance::Crouched,
            Stance::Crouched => Stance::Standing,
        }
    }

    /// Half the height of the Stance's capsule: how far its centre sits above the feet.
    pub fn half_height(self) -> f32 {
        match self {
            Stance::Standing => PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0,
            Stance::Crouched => PLAYER_RADIUS + CROUCH_COLLIDER_LENGTH / 2.0,
        }
    }

    /// The eye's height above the feet in this Stance.
    pub fn eye_height(self) -> f32 {
        match self {
            Stance::Standing => PLAYER_EYE_HEIGHT + Stance::Standing.half_height(),
            Stance::Crouched => CROUCH_EYE_HEIGHT,
        }
    }

    /// The capsule the Stance's body is.
    pub fn collider(self) -> Collider {
        match self {
            Stance::Standing => Collider::capsule(PLAYER_RADIUS, PLAYER_CAPSULE_LENGTH),
            Stance::Crouched => Collider::capsule(PLAYER_RADIUS, CROUCH_COLLIDER_LENGTH),
        }
    }
}

/// The Crouch key resolved into the Stance the Player wants, and the memory behind it.
/// `fill_move_input` runs it for the `LocalPlayer` each tick, by the Stance Keys setting. The
/// Toggle or Hold choice never crosses the wire: only the resolved target rides the `MoveInput`,
/// so the Host replays the same presses to the same Stance.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct StanceKeys {
    /// The Stance wanted.
    pub target: Stance,
    /// The Crouch, Jump and Sprint keys last tick, for edges.
    held: [bool; 3],
    /// A Jump stand-up: the Crouch key gives way until it is released, and the jump that stood
    /// the Player up doesn't go on to jump them.
    yielded: bool,
    /// A jump that stood the Player up, still held.
    standing_jump: bool,
}

impl Default for StanceKeys {
    fn default() -> Self {
        Self { target: Stance::Standing, held: [false; 3], yielded: false, standing_jump: false }
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

    /// One tick of the Crouch, Jump and Sprint keys into the Stance wanted, given whether the
    /// Player is `grounded` and `down`. Returns whether the jump passes this tick: a jump that
    /// stands the Player up, or one held from it, doesn't jump them.
    ///
    /// Stance input is dead while Down: the target comes back Standing. Airborne presses are
    /// ignored; a Hold key still held lands the Player in its Stance when they do.
    pub fn resolve(
        &mut self,
        mode: StanceMode,
        crouch: bool,
        jump: bool,
        sprint: bool,
        grounded: bool,
        down: bool,
    ) -> bool {
        let [crouch_was, jump_was, sprint_was] = self.held;
        self.held = [crouch, jump, sprint];
        if down {
            self.target = Stance::Standing;
            return jump;
        }
        if !jump {
            self.standing_jump = false;
        }
        match mode {
            StanceMode::Toggle => {
                if grounded && crouch && !crouch_was {
                    self.target = self.target.other();
                }
                if grounded
                    && self.target == Stance::Crouched
                    && (jump && !jump_was || sprint && !sprint_was)
                {
                    self.target = Stance::Standing;
                    self.standing_jump = jump && !jump_was;
                }
            }
            StanceMode::Hold => {
                if !crouch {
                    self.yielded = false;
                }
                if grounded && self.target == Stance::Crouched && jump && !jump_was {
                    self.yielded = true;
                    self.standing_jump = true;
                }
                if sprint {
                    // Sprint wins over the held key, while it is held.
                    self.target = Stance::Standing;
                } else if self.yielded {
                    self.target = Stance::Standing;
                } else {
                    self.target = if crouch { Stance::Crouched } else { Stance::Standing };
                }
            }
        }
        jump && !(self.standing_jump && jump)
    }
}

#[derive(Component)]
pub struct Player;

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
/// off the network. The Stance the Player wants arrives resolved: `crouch` is the target stance,
/// already past the Toggle or Hold setting.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct MoveInput {
    /// Move axes, each -1, 0 or 1: x is right, y is forward.
    pub axes: Vec2,
    pub jump: bool,
    pub sprint: bool,
    pub aim: bool,
    /// The Player wants the Crouched Stance (Standing when false).
    pub crouch: bool,
    /// Which way the Player faces, in radians about +Y (0 faces -Z).
    pub yaw: f32,
    /// How far the Player looks up (positive) or down, in radians.
    pub pitch: f32,
}

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
    /// Seconds since the current stance change began; `STANCE_CROUCH_SECS` or more once settled.
    pub(crate) since_stance_change: f32,
    /// The eye's height above the feet, gliding between stance eye heights.
    pub(crate) eye: f32,
    /// The speed the eye glides at, set when a stance change begins.
    pub(crate) eye_rate: f32,
}

impl Locomotion {
    pub fn grounded(&self) -> bool {
        self.grounded
    }

    /// Puts the body in the Standing Stance at once, as a Respawn, a Teleporter ride or the start
    /// of a Run does. The caller sets the collider to match.
    pub(crate) fn stand(&mut self) {
        self.stance = Stance::Standing;
        self.since_stance_change = STANCE_CROUCH_SECS;
        self.eye = Stance::Standing.eye_height();
        self.eye_rate = 0.0;
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
            stamina: SPRINT_DURATION_SECS,
            winded: false,
            stance: Stance::Standing,
            since_stance_change: STANCE_CROUCH_SECS,
            eye: Stance::Standing.eye_height(),
            eye_rate: 0.0,
        }
    }
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
}

/// Places the camera's `Eye` from the stance glide: the eye's height above the feet, less the
/// capsule's half height. While Down, Last Stand owns the eye instead (`lower_eye`).
pub(crate) fn place_eye(
    player: Single<(&Locomotion, Has<LastStand>), With<LocalPlayer>>,
    mut eye: Single<&mut Eye, With<PlayerCamera>>,
) {
    let (locomotion, last_stand) = player.into_inner();
    if !last_stand {
        eye.height = locomotion.eye - locomotion.stance.half_height();
    }
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
            .init_resource::<FovWarp>()
            .init_resource::<RunModifiers>()
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
                (teleport_player, fill_move_input, move_player).chain().run_if(world_runs),
            )
            .add_systems(RunFixedMainLoop, look.run_if(input_runs).in_set(LookSystems))
            .add_systems(
                Update,
                (take_hits, regenerate).in_set(HostSimulation).run_if(world_runs),
            )
            // The camera rides the stance glide; Last Stand owns the eye while Down, so this
            // runs after its `lower_eye`, and the sway reads what the two of them left.
            .add_systems(Update, place_eye.after(lower_eye))
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
fn run_state(run: &[Modifier]) -> impl Bundle {
    (
        Points::starting(run),
        RunStats::default(),
        PlayerZone::default(),
        Shellshocked::default(),
        Cooldowns::default(),
        SparesUsed::default(),
    )
}

/// Where a Seat's Player starts: the Seat's spawn point on the Map, with the Player standing on it.
fn seat_spawn(seat: Seat) -> Transform {
    let half_height = PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0;
    Transform::from_translation(seat_start(seat.0) + Vec3::Y * (half_height + 0.05))
}

/// Everything a Player is at the start of a Run, in `seat`, whoever drives them. What makes one the
/// `LocalPlayer` (the camera and view model) is added by the caller.
pub(crate) fn player_body(seat: Seat, name: &str, run: &[Modifier]) -> impl Bundle {
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
            (Pose::default(), Shots::default(), HeldWeapon::default()),
        ),
        Visibility::default(),
        Loadout::new(STARTING_WEAPON),
        Modifiers(run.to_vec()),
        (Knife::default(), Equipment::default(), run_state(run), PlankTally::default()),
        (
            seat_spawn(seat),
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
    party: Option<Res<Party>>,
) {
    // Solo is a party of one. In a Co-op Run every Seat has a Player, and only this machine's is
    // the `LocalPlayer`.
    let (seats, local) = match &party {
        Some(party) => (party.seats().to_vec(), party.local),
        None => (vec![(Seat(0), "Player".to_string())], Seat(0)),
    };
    for (seat, name) in seats {
        if seat != local {
            commands.spawn(player_body(seat, &name, &[]));
            continue;
        }
        commands
            .spawn((player_body(seat, &name, &run.0), LocalPlayer))
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

/// A debug convenience: puts a second Player in Seat 1, standing on its spawn point and driven
/// only through `MoveInput` and Player-tagged messages. It has no camera, no `LocalPlayer` and no
/// view model; `teammate` gives it a body. It does nothing if Seat 1 is
/// taken. The Run's type is already fixed, so a Player added here doesn't make it Co-op.
pub fn spawn_debug_player(
    mut commands: Commands,
    run: Res<RunModifiers>,
    seats: Query<&Seat, With<Player>>,
) {
    let seat = Seat(1);
    if seats.iter().any(|&taken| taken == seat) {
        return;
    }
    commands.spawn(player_body(seat, "Player 2", &run.0));
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
    mut player: Single<&mut Transform, With<LocalPlayer>>,
    mut camera: Single<&mut Transform, (With<PlayerCamera>, Without<LocalPlayer>)>,
) {
    let delta = settings.look_delta(mouse.delta);
    player.rotate_y(-delta.x);

    const PITCH_LIMIT: f32 = FRAC_PI_2 - 0.01;
    let (_, pitch, _) = camera.rotation.to_euler(EulerRot::YXZ);
    let pitch = (pitch - delta.y).clamp(-PITCH_LIMIT, PITCH_LIMIT);
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
/// Crouch key runs through the Stance resolver, so the Stance wanted arrives already resolved
/// from the Toggle or Hold setting.
pub(crate) fn fill_move_input(
    controls: Controls,
    settings: Res<Settings>,
    player: Single<(&Transform, &mut MoveInput, &mut StanceKeys, &Locomotion, Has<LastStand>), With<LocalPlayer>>,
    camera: Single<&Transform, (With<PlayerCamera>, Without<LocalPlayer>)>,
) {
    let (transform, mut input, mut keys, locomotion, last_stand) = player.into_inner();
    let forward = controls.pressed(Action::Forward) as i8 - controls.pressed(Action::Back) as i8;
    let right = controls.pressed(Action::Right) as i8 - controls.pressed(Action::Left) as i8;
    let jump = keys.resolve(
        settings.stance_keys,
        controls.pressed(Action::Crouch),
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
        sprint: controls.pressed(Action::Sprint),
        aim: controls.pressed(Action::Aim),
        crouch: keys.target == Stance::Crouched,
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
) {
    for (entity, mut transform, mut locomotion, input, loadout, modifiers, drinking, last_stand) in &mut players {
        let mut context = MoveContext { entity, loadout, modifiers, drinking, last_stand };
        if let Some(stance) = step_move(&mut transform.translation, &mut locomotion, input, &mut context, time.delta(), &move_and_slide) {
            commands.entity(entity).insert(stance.collider());
            if let Ok(mut easing) = easings.get_mut(entity) {
                easing.start = Some(transform.translation);
            }
        }
    }
}

/// One tick of a Player's movement: advances `position` and `Locomotion` by `delta` under
/// `input`. It reads no keyboard and no clock, so given the same start, input and world it
/// reaches the same place every time, which is what lets a client replay inputs through it.
/// Returns the Stance the body took this tick, if it changed.
pub fn step_move(
    position: &mut Vec3,
    locomotion: &mut Locomotion,
    input: &MoveInput,
    context: &mut MoveContext,
    delta: Duration,
    move_and_slide: &MoveAndSlide,
) -> Option<Stance> {
    let MoveContext { entity, loadout, modifiers, drinking, last_stand } = *context;
    let dt = delta.as_secs_f32();
    let rotation = Quat::from_rotation_y(input.yaw);

    let wish = input.axes.normalize_or_zero();

    // Stance: the input carries the Stance wanted, resolved from the keys. The body takes it at
    // the start of a transition, both ways: going down shrinks the capsule at once, feet staying
    // put; getting up passes the ceiling check first, then grows. Nothing changes in the air.
    let target = if input.crouch && !last_stand { Stance::Crouched } else { Stance::Standing };
    let mut changed = None;
    if locomotion.stance != target
        && locomotion.grounded
        && (target == Stance::Crouched || room_to_stand(move_and_slide, entity, *position))
    {
        position.y += target.half_height() - locomotion.stance.half_height();
        locomotion.eye_rate = (target.eye_height() - locomotion.eye).abs() / STANCE_CROUCH_SECS;
        locomotion.stance = target;
        locomotion.since_stance_change = 0.0;
        changed = Some(target);
    }
    // The step works from the Stance's own shape; the caller puts the collider component into it.
    let shape = locomotion.stance.collider();
    locomotion.since_stance_change += dt;
    // The camera keeps gliding to the new stance's eye for the whole transition.
    locomotion.eye = move_towards(locomotion.eye, locomotion.stance.eye_height(), locomotion.eye_rate * dt);

    // Sprinting only makes sense running forwards on the ground, and it runs out. Aiming stops
    // it, there's no sprinting with a Perk in hand or in Last Stand, and only Standing sprints;
    // from Crouched, Sprint stands and sprints at once.
    let aiming = input.aim && loadout.can_aim();
    let wants_sprint = input.sprint
        && input.axes.y > 0.0
        && locomotion.grounded
        && !aiming
        && !drinking
        && !last_stand
        && locomotion.stance == Stance::Standing;
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
    let crouched = locomotion.stance == Stance::Crouched || locomotion.since_stance_change < STANCE_CROUCH_SECS;
    let base = if last_stand {
        PLAYER_WALK_SPEED * LAST_STAND_CRAWL_MULT
    } else if locomotion.sprinting {
        PLAYER_SPRINT_SPEED
    } else if crouched {
        PLAYER_WALK_SPEED * CROUCH_SPEED_MULT
    } else {
        PLAYER_WALK_SPEED
    };
    let speed = loadout.move_speed(base, &modifiers.0);

    let wish_world = rotation * Vec3::new(wish.x, 0.0, -wish.y) * speed;
    let horizontal = Vec3::new(locomotion.velocity.x, 0.0, locomotion.velocity.z);
    let horizontal = horizontal.move_towards(wish_world, PLAYER_ACCELERATION * dt);
    // Gravity only pulls in the air; on the floor it would just push the Player into it, and
    // slowly down the ramp while standing still.
    let mut vertical = if locomotion.grounded { 0.0 } else { locomotion.velocity.y - GRAVITY * dt };
    // No jumping from a crawl, and no jumping out of a Stance: Jump while Crouched stands the
    // Player up instead (the resolver turned the press into the stand-up).
    let jumped = locomotion.grounded && input.jump && !last_stand && locomotion.stance == Stance::Standing;
    if jumped {
        vertical = PLAYER_JUMP_SPEED;
    }
    let velocity = horizontal + Vec3::Y * vertical;

    let config = MoveAndSlideConfig::default();
    let filter = SpatialQueryFilter::from_mask([GameLayer::World, GameLayer::Zombie, GameLayer::Window])
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

    *position = new_position;
    locomotion.velocity = velocity;
    locomotion.grounded = grounded;
    changed
}

/// Whether the standing capsule fits where the Player is, feet staying put: the ceiling check
/// that can refuse a stand-up. The floor sits a skin's width below the feet, so it never counts.
fn room_to_stand(move_and_slide: &MoveAndSlide, entity: Entity, position: Vec3) -> bool {
    let stood = position + Vec3::Y * (Stance::Standing.half_height() - Stance::Crouched.half_height());
    let filter = SpatialQueryFilter::from_mask([GameLayer::World, GameLayer::Zombie, GameLayer::Window])
        .with_excluded_entities([entity]);
    let mut room = true;
    move_and_slide.intersections(
        &Stance::Standing.collider(),
        stood,
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
    mut players: Query<(&mut Health, Has<LastStand>, Has<AwaitingRespawn>), With<Player>>,
) {
    for hit in hits.read() {
        let Ok((mut health, last_stand, awaiting)) = players.get_mut(hit.player) else {
            continue;
        };
        if last_stand || awaiting {
            continue;
        }
        health.current -= hit.damage;
        health.since_hit = 0.0;
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
    fn toggle_flips_on_each_press_and_jump_and_sprint_stand_you_up() {
        use crate::settings::StanceMode::*;
        let mut keys = StanceKeys::default();
        assert_eq!(keys.target, Stance::Standing);
        // A press flips down; letting go changes nothing.
        keys.resolve(Toggle, true, false, false, true, false);
        assert_eq!(keys.target, Stance::Crouched);
        keys.resolve(Toggle, false, false, false, true, false);
        assert_eq!(keys.target, Stance::Crouched);
        // Another press flips back up.
        keys.resolve(Toggle, true, false, false, true, false);
        assert_eq!(keys.target, Stance::Standing);

        // Jump stands the Player up instead of jumping, and doesn't jump while held.
        keys.resolve(Toggle, false, false, false, true, false);
        keys.resolve(Toggle, true, false, false, true, false);
        assert!(!keys.resolve(Toggle, false, true, false, true, false), "the stand-up jump jumped");
        assert_eq!(keys.target, Stance::Standing);
        assert!(!keys.resolve(Toggle, false, true, false, true, false), "the held jump jumped");
        // Once the jump is let go, the next press jumps again.
        keys.resolve(Toggle, false, false, false, true, false);
        assert!(keys.resolve(Toggle, false, true, false, true, false));

        // Sprint stands and sprints at once; holding it doesn't stop a later crouch.
        keys.resolve(Toggle, true, false, false, true, false);
        keys.resolve(Toggle, false, false, true, true, false);
        assert_eq!(keys.target, Stance::Standing);
        keys.resolve(Toggle, true, false, true, true, false);
        assert_eq!(keys.target, Stance::Crouched, "the held sprint kept standing");
    }

    #[test]
    fn hold_keeps_you_down_while_held_and_jump_and_sprint_win() {
        use crate::settings::StanceMode::*;
        let mut keys = StanceKeys::default();
        // Held keeps the Player down; letting go brings them up.
        keys.resolve(Hold, true, false, false, true, false);
        assert_eq!(keys.target, Stance::Crouched);
        keys.resolve(Hold, false, false, false, true, false);
        assert_eq!(keys.target, Stance::Standing);

        // Jump stands the Player up, and the key gives way until released and pressed again.
        keys.resolve(Hold, true, false, false, true, false);
        assert!(!keys.resolve(Hold, true, true, false, true, false), "the stand-up jump jumped");
        assert_eq!(keys.target, Stance::Standing);
        keys.resolve(Hold, true, false, false, true, false);
        assert_eq!(keys.target, Stance::Standing, "the held key went back down");
        keys.resolve(Hold, false, false, false, true, false);
        keys.resolve(Hold, true, false, false, true, false);
        assert_eq!(keys.target, Stance::Crouched, "released and pressed again didn't go down");

        // Sprint wins over the held key while it is held, and gives the key back when let go.
        keys.resolve(Hold, false, false, true, true, false);
        assert_eq!(keys.target, Stance::Standing);
        keys.resolve(Hold, true, false, false, true, false);
        keys.resolve(Hold, true, false, true, true, false);
        assert_eq!(keys.target, Stance::Standing, "the held key beat the sprint");
        keys.resolve(Hold, true, false, false, true, false);
        assert_eq!(keys.target, Stance::Crouched, "the sprint's release didn't hand the key back");
    }

    #[test]
    fn airborne_presses_are_ignored_and_a_down_resets_the_target() {
        use crate::settings::StanceMode::*;
        for mode in [Toggle, Hold] {
            let mut keys = StanceKeys::default();
            // In the air nothing flips; a held key still lands them in its Stance when they do.
            keys.resolve(mode, true, false, false, false, false);
            assert_eq!(keys.target, if mode == Hold { Stance::Crouched } else { Stance::Standing }, "{mode:?}");
            let mut down = StanceKeys { target: Stance::Crouched, ..default() };
            // Stance input is dead while Down: the target comes back Standing.
            assert!(down.resolve(mode, true, true, true, true, true), "the Down took the jump");
            assert_eq!(down.target, Stance::Standing);
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
