use std::f32::consts::FRAC_PI_2;

use avian3d::prelude::*;
use bevy::input::common_conditions::input_just_pressed;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};

use crate::equipment::Equipment;
use crate::last_stand::LastStand;
use crate::layers::GameLayer;
use crate::map::PLAYER_START;
use crate::modifier::{Modifiers, Stat, resolve};
use crate::perk::Drinking;
use crate::settings::Settings;
use crate::state::{GameState, InRun, Pause};
use crate::tuning::*;
use crate::weapon::{Knife, Loadout};

#[derive(Component)]
pub struct Player;

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
    since_hit: f32,
}

#[derive(Component)]
pub struct Locomotion {
    velocity: Vec3,
    grounded: bool,
    pub sprinting: bool,
    /// Seconds of sprint left.
    stamina: f32,
    /// Sprint ran out; it can't start again until fully recharged.
    winded: bool,
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

/// A Zombie's swing connected.
#[derive(Message)]
pub struct PlayerHit {
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
            .add_systems(OnEnter(GameState::Playing), spawn_player)
            .add_systems(OnEnter(Pause::Running), grab_cursor)
            .add_systems(OnExit(Pause::Running), release_cursor)
            // Settings apply immediately, even from Pause.
            .add_systems(Update, apply_fov)
            .add_systems(FixedUpdate, move_player.run_if(in_state(Pause::Running)))
            .add_systems(RunFixedMainLoop, look.run_if(in_state(Pause::Running)).in_set(LookSystems))
            .add_systems(
                Update,
                (take_hits, regenerate).run_if(in_state(Pause::Running)),
            )
            // The OS can drop the grab (e.g. on focus loss); clicking takes it back.
            .add_systems(
                Update,
                grab_cursor
                    .run_if(in_state(Pause::Running))
                    .run_if(input_just_pressed(MouseButton::Left)),
            );
    }
}

fn spawn_player(mut commands: Commands, settings: Res<Settings>) {
    let half_height = PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0;
    commands
        .spawn((
            InRun,
            Name::new("Player"),
            Player,
            Health {
                current: PLAYER_MAX_HEALTH,
                max: PLAYER_MAX_HEALTH,
                since_hit: HEALTH_REGEN_DELAY_SECS,
            },
            Locomotion::default(),
            Visibility::default(),
            Loadout::new(STARTING_WEAPON),
            Modifiers::default(),
            (Knife::default(), Equipment::default()),
            Transform::from_translation(PLAYER_START + Vec3::Y * (half_height + 0.05)),
            RigidBody::Kinematic,
            CustomPositionIntegration,
            TranslationInterpolation,
            Collider::capsule(PLAYER_RADIUS, PLAYER_CAPSULE_LENGTH),
            CollisionLayers::new(GameLayer::Player, LayerMask::ALL),
        ))
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

fn grab_cursor(mut cursor: Single<&mut CursorOptions>) {
    cursor.visible = false;
    cursor.grab_mode = CursorGrabMode::Locked;
}

fn release_cursor(mut cursor: Single<&mut CursorOptions>) {
    cursor.visible = true;
    cursor.grab_mode = CursorGrabMode::None;
}

/// The FOV setting, narrowed while aiming down the sights.
fn apply_fov(
    settings: Res<Settings>,
    loadout: Single<&Loadout, With<Player>>,
    mut cameras: Query<&mut Projection, With<PlayerCamera>>,
) {
    let fov = settings.fov_radians() * loadout.fov_factor();
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
    mut player: Single<&mut Transform, With<Player>>,
    mut camera: Single<&mut Transform, (With<PlayerCamera>, Without<Player>)>,
) {
    let delta = settings.look_delta(mouse.delta);
    player.rotate_y(-delta.x);

    const PITCH_LIMIT: f32 = FRAC_PI_2 - 0.01;
    let (_, pitch, _) = camera.rotation.to_euler(EulerRot::YXZ);
    let pitch = (pitch - delta.y).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    camera.rotation = Quat::from_rotation_x(pitch);
}

fn move_player(
    player: Single<
        (
            Entity,
            &mut Transform,
            &mut Locomotion,
            &Collider,
            &Loadout,
            &Modifiers,
            Has<Drinking>,
            Has<LastStand>,
        ),
        With<Player>,
    >,
    input: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    move_and_slide: MoveAndSlide,
    time: Res<Time>,
) {
    let (entity, mut transform, mut locomotion, collider, loadout, modifiers, drinking, last_stand) =
        player.into_inner();
    let dt = time.delta_secs();

    let forward = input.pressed(KeyCode::KeyW) as i8 - input.pressed(KeyCode::KeyS) as i8;
    let right = input.pressed(KeyCode::KeyD) as i8 - input.pressed(KeyCode::KeyA) as i8;
    let wish = Vec2::new(right as f32, forward as f32).normalize_or_zero();

    // Sprinting only makes sense running forwards on the ground, and it runs out. Aiming stops it,
    // and there's no sprinting with a Perk in hand or in Last Stand.
    let aiming = mouse.pressed(MouseButton::Right) && loadout.can_aim();
    let wants_sprint = input.pressed(KeyCode::ShiftLeft)
        && forward > 0
        && locomotion.grounded
        && !aiming
        && !drinking
        && !last_stand;
    locomotion.sprinting = wants_sprint && !locomotion.winded;
    if locomotion.sprinting {
        locomotion.stamina -= dt;
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

    let wish_world = transform.rotation * Vec3::new(wish.x, 0.0, -wish.y) * speed;
    let horizontal = Vec3::new(locomotion.velocity.x, 0.0, locomotion.velocity.z);
    let horizontal = horizontal.move_towards(wish_world, PLAYER_ACCELERATION * dt);
    // Gravity only pulls in the air; on the floor it would just push the Player into it, and
    // slowly down the ramp while standing still.
    let mut vertical = if locomotion.grounded { 0.0 } else { locomotion.velocity.y - GRAVITY * dt };
    // No jumping from a crawl.
    let jumped = locomotion.grounded && input.pressed(KeyCode::Space) && !last_stand;
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
        transform.translation,
        transform.rotation,
        velocity,
        time.delta(),
        &config,
        &filter,
        |hit| {
            if hit.normal.y > PLAYER_GROUND_NORMAL_Y {
                grounded = true;
            }
            MoveAndSlideHitResponse::Accept
        },
    );
    let mut position = out.position;
    let mut velocity = out.projected_velocity;

    // Move-and-slide only touches the floor on steps where gravity's pull reaches it through the
    // skin, so on its own `grounded` flickered every other step on flat floor (and sprint with
    // it), and the ramp's slope carried the Player up off it. A short probe down finds the floor
    // every step and keeps the Player on it.
    if !grounded
        && probes_for_ground(locomotion.grounded, jumped, vertical)
        && let Some(hit) = move_and_slide.cast_move(
            collider,
            position,
            transform.rotation,
            Vec3::NEG_Y * PLAYER_GROUND_PROBE,
            config.skin_width,
            &filter,
        )
        && hit.normal1.y > PLAYER_GROUND_NORMAL_Y
    {
        position.y -= hit.distance;
        grounded = true;
    }
    // A jump leaves the floor even if it's still within reach this step.
    let grounded = grounded && !jumped;
    // On the floor the Player's speed carries on along the ground, so falling stops and walking up
    // the ramp doesn't launch them off the top of it.
    if grounded {
        velocity = velocity.with_y(0.0).normalize_or_zero() * velocity.length();
    }

    transform.translation = position;
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

/// Zombie swipes take Health. Nothing hurts the Player in Last Stand. Reaching zero is a Down,
/// which the Last Stand module handles.
pub fn take_hits(mut hits: MessageReader<PlayerHit>, player: Single<(&mut Health, Has<LastStand>), With<Player>>) {
    let (mut health, last_stand) = player.into_inner();
    for hit in hits.read() {
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
    time: Res<Time>,
) {
    let (mut health, modifiers) = player.into_inner();
    health.set_max(resolve(Stat::MaxHealth, PLAYER_MAX_HEALTH, &modifiers.0));
    let dt = time.delta_secs();
    health.since_hit += dt;
    if health.since_hit >= resolve(Stat::HealthRegenDelay, HEALTH_REGEN_DELAY_SECS, &modifiers.0) {
        let rate = health.max / HEALTH_REGEN_FULL_SECS;
        health.current = (health.current + rate * dt).min(health.max);
    }
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
    fn raising_max_health_leaves_current_health_to_regen() {
        let mut h = health(100.0, 100.0);
        h.set_max(160.0);
        assert_eq!((h.current, h.max), (100.0, 160.0));
    }

    #[test]
    fn lowering_max_health_caps_current_health() {
        let mut h = health(150.0, 160.0);
        h.set_max(100.0);
        assert_eq!((h.current, h.max), (100.0, 100.0));
    }
}
