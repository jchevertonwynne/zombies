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
use crate::last_stand::LastStand;
use crate::layers::GameLayer;
use crate::map::seat_start;
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
use crate::settings::Settings;
use crate::state::{GameState, InRun, Pause, input_runs, world_runs};
use crate::teammate::{HeldWeapon, Pose, Shots};
use crate::tuning::*;
use crate::weapon::{Knife, Loadout};

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
/// off the network. Stance (crouch, prone) joins it here.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct MoveInput {
    /// Move axes, each -1, 0 or 1: x is right, y is forward.
    pub axes: Vec2,
    pub jump: bool,
    pub sprint: bool,
    pub aim: bool,
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
}

impl Locomotion {
    pub fn grounded(&self) -> bool {
        self.grounded
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
        (Locomotion::default(), MoveInput::default(), Pose::default(), Shots::default(), HeldWeapon::default()),
        Visibility::default(),
        Loadout::new(STARTING_WEAPON),
        Modifiers(run.to_vec()),
        (Knife::default(), Equipment::default(), run_state(run), PlankTally::default()),
        seat_spawn(seat),
        RigidBody::Kinematic,
        CustomPositionIntegration,
        TranslationInterpolation,
        Collider::capsule(PLAYER_RADIUS, PLAYER_CAPSULE_LENGTH),
        CollisionLayers::new(GameLayer::Player, LayerMask::ALL),
    )
}

/// Spawns the Players a Run starts with. The Players are in the world once this has run (see
/// `RunType`).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SpawnPlayers;

fn spawn_player(mut commands: Commands, settings: Res<Settings>, run: Res<RunModifiers>) {
    commands
        .spawn((player_body(Seat(0), "Player", &run.0), LocalPlayer))
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

fn grab_cursor(mut cursor: Single<&mut CursorOptions>) {
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
/// spot too, so no frame shows the Player partway there.
fn teleport_player(
    mut pending: ResMut<PendingTeleports>,
    mut players: Query<(&mut Transform, &mut Locomotion, &mut TranslationEasingState), With<Player>>,
) {
    for to in pending.0.drain(..) {
        let Ok((mut transform, mut locomotion, mut easing)) = players.get_mut(to.player) else {
            continue;
        };
        transform.translation = to.feet + Vec3::Y * (PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0 + 0.05);
        if let Some(facing) = to.facing {
            transform.rotation = facing;
        }
        easing.start = Some(transform.translation);
        locomotion.velocity = Vec3::ZERO;
        locomotion.grounded = false;
    }
}

/// Fills the local Player's `MoveInput` for the tick from the keyboard and the way they're
/// facing: the yaw is the Player's, the pitch the camera's, both as `look` has turned them.
pub(crate) fn fill_move_input(
    controls: Controls,
    player: Single<(&Transform, &mut MoveInput), With<LocalPlayer>>,
    camera: Single<&Transform, (With<PlayerCamera>, Without<LocalPlayer>)>,
) {
    let (transform, mut input) = player.into_inner();
    let forward = controls.pressed(Action::Forward) as i8 - controls.pressed(Action::Back) as i8;
    let right = controls.pressed(Action::Right) as i8 - controls.pressed(Action::Left) as i8;
    let (yaw, _, _) = transform.rotation.to_euler(EulerRot::YXZ);
    let (_, pitch, _) = camera.rotation.to_euler(EulerRot::YXZ);
    *input = MoveInput {
        axes: Vec2::new(right as f32, forward as f32),
        jump: controls.pressed(Action::Jump),
        sprint: controls.pressed(Action::Sprint),
        aim: controls.pressed(Action::Aim),
        yaw,
        pitch,
    };
}

/// What `step_move` needs to know about a Player besides their input and `Locomotion`: who they
/// are, their shape, and what they're carrying and doing.
pub struct MoveContext<'a> {
    pub entity: Entity,
    pub collider: &'a Collider,
    pub loadout: &'a Loadout,
    pub modifiers: &'a Modifiers,
    pub drinking: bool,
    pub last_stand: bool,
}

/// Runs this tick's `MoveInput` for every Player but a client's puppets, which move as the Host
/// says. The step itself is `step_move`.
pub(crate) fn move_player(
    mut players: Query<
        (Entity, &mut Transform, &mut Locomotion, &MoveInput, &Collider, &Loadout, &Modifiers, Has<Drinking>, Has<LastStand>),
        (With<Player>, Without<Puppet>),
    >,
    move_and_slide: MoveAndSlide,
    time: Res<Time>,
) {
    for (entity, mut transform, mut locomotion, input, collider, loadout, modifiers, drinking, last_stand) in
        &mut players
    {
        let context = MoveContext { entity, collider, loadout, modifiers, drinking, last_stand };
        step_move(&mut transform.translation, &mut locomotion, input, &context, time.delta(), &move_and_slide);
    }
}

/// One tick of a Player's movement: advances `position` and `Locomotion` by `delta` under
/// `input`. It reads no keyboard and no clock, so given the same start, input and world it
/// reaches the same place every time, which is what lets a client replay inputs through it.
pub fn step_move(
    position: &mut Vec3,
    locomotion: &mut Locomotion,
    input: &MoveInput,
    context: &MoveContext,
    delta: Duration,
    move_and_slide: &MoveAndSlide,
) {
    let MoveContext { entity, collider, loadout, modifiers, drinking, last_stand } = *context;
    let dt = delta.as_secs_f32();
    let rotation = Quat::from_rotation_y(input.yaw);

    let wish = input.axes.normalize_or_zero();

    // Sprinting only makes sense running forwards on the ground, and it runs out. Aiming stops it,
    // and there's no sprinting with a Perk in hand or in Last Stand.
    let aiming = input.aim && loadout.can_aim();
    let wants_sprint = input.sprint
        && input.axes.y > 0.0
        && locomotion.grounded
        && !aiming
        && !drinking
        && !last_stand;
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
    let base = if last_stand {
        PLAYER_WALK_SPEED * LAST_STAND_CRAWL_MULT
    } else if locomotion.sprinting {
        PLAYER_SPRINT_SPEED
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
    // No jumping from a crawl.
    let jumped = locomotion.grounded && input.jump && !last_stand;
    if jumped {
        vertical = PLAYER_JUMP_SPEED;
    }
    let velocity = horizontal + Vec3::Y * vertical;

    let config = MoveAndSlideConfig::default();
    let filter = SpatialQueryFilter::from_mask([GameLayer::World, GameLayer::Zombie, GameLayer::Window])
        .with_excluded_entities([entity]);
    let mut grounded = false;
    let out = move_and_slide.move_and_slide(
        collider,
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
            collider,
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
pub fn take_hits(mut hits: MessageReader<PlayerHit>, mut players: Query<(&mut Health, Has<LastStand>), With<Player>>) {
    for hit in hits.read() {
        let Ok((mut health, last_stand)) = players.get_mut(hit.player) else {
            continue;
        };
        if last_stand {
            continue;
        }
        health.current -= hit.damage;
        health.since_hit = 0.0;
    }
}

/// Health stays empty through a Last Stand; the Self-Revive refills it.
fn regenerate(
    player: Single<(&mut Health, &Modifiers), (With<Player>, Without<LastStand>)>,
    round: Option<Res<Round>>,
    time: Res<Time>,
) {
    let (mut health, modifiers) = player.into_inner();
    health.set_max(resolve(Stat::MaxHealth, PLAYER_MAX_HEALTH, &modifiers.0));
    let dt = time.delta_secs();
    health.since_hit += dt;
    if health.since_hit >= resolve(Stat::HealthRegenDelay, HEALTH_REGEN_DELAY_SECS, &modifiers.0) {
        let in_round = round.is_some_and(|round| round.phase == RoundPhase::Active);
        let rate = regen_rate(health.max, in_round, &modifiers.0);
        health.current = (health.current + rate * dt).min(health.max);
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
mod two_players;

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
