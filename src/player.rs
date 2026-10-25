use std::f32::consts::FRAC_PI_2;

use avian3d::prelude::*;
use bevy::input::common_conditions::input_just_pressed;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};

use crate::equipment::Equipment;
use crate::layers::GameLayer;
use crate::map::PLAYER_START;
use crate::modifier::Modifiers;
use crate::settings::Settings;
use crate::state::{GameState, InRun, Pause};
use crate::tuning::*;
use crate::weapon::{Knife, Loadout};

#[derive(Component)]
pub struct Player;

/// The first-person camera, a child of the Player. It carries the pitch; the Player carries the yaw.
#[derive(Component)]
pub struct PlayerCamera;

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

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PlayerHit>()
            .add_systems(OnEnter(GameState::Playing), spawn_player)
            .add_systems(OnEnter(Pause::Running), grab_cursor)
            .add_systems(OnExit(Pause::Running), release_cursor)
            // Settings apply immediately, even from Pause.
            .add_systems(Update, apply_fov)
            .add_systems(FixedUpdate, move_player.run_if(in_state(Pause::Running)))
            .add_systems(
                Update,
                (look, take_hits, regenerate).run_if(in_state(Pause::Running)),
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
            Camera3d::default(),
            Projection::Perspective(PerspectiveProjection {
                fov: settings.fov_radians(),
                ..default()
            }),
            DistanceFog {
                color: Color::srgb(0.02, 0.02, 0.04),
                falloff: FogFalloff::Linear {
                    start: 10.0,
                    end: 45.0,
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
    player: Single<(Entity, &mut Transform, &mut Locomotion, &Collider, &Loadout, &Modifiers), With<Player>>,
    input: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    move_and_slide: MoveAndSlide,
    time: Res<Time>,
) {
    let (entity, mut transform, mut locomotion, collider, loadout, modifiers) = player.into_inner();
    let dt = time.delta_secs();

    let forward = input.pressed(KeyCode::KeyW) as i8 - input.pressed(KeyCode::KeyS) as i8;
    let right = input.pressed(KeyCode::KeyD) as i8 - input.pressed(KeyCode::KeyA) as i8;
    let wish = Vec2::new(right as f32, forward as f32).normalize_or_zero();

    // Sprinting only makes sense running forwards on the ground, and it runs out. Aiming stops it.
    let aiming = mouse.pressed(MouseButton::Right) && loadout.can_aim();
    let wants_sprint = input.pressed(KeyCode::ShiftLeft) && forward > 0 && locomotion.grounded && !aiming;
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
    let base = if locomotion.sprinting { PLAYER_SPRINT_SPEED } else { PLAYER_WALK_SPEED };
    let speed = loadout.move_speed(base, &modifiers.0);

    let wish_world = transform.rotation * Vec3::new(wish.x, 0.0, -wish.y) * speed;
    let horizontal = Vec3::new(locomotion.velocity.x, 0.0, locomotion.velocity.z);
    let horizontal = horizontal.move_towards(wish_world, PLAYER_ACCELERATION * dt);
    let mut vertical = locomotion.velocity.y - GRAVITY * dt;
    if locomotion.grounded && input.pressed(KeyCode::Space) {
        vertical = PLAYER_JUMP_SPEED;
    }
    let velocity = horizontal + Vec3::Y * vertical;

    let mut grounded = false;
    let out = move_and_slide.move_and_slide(
        collider,
        transform.translation,
        transform.rotation,
        velocity,
        time.delta(),
        &MoveAndSlideConfig::default(),
        &SpatialQueryFilter::from_mask([GameLayer::World, GameLayer::Zombie])
            .with_excluded_entities([entity]),
        |hit| {
            if hit.normal.y > 0.7 {
                grounded = true;
            }
            MoveAndSlideHitResponse::Accept
        },
    );
    transform.translation = out.position;
    locomotion.velocity = out.projected_velocity;
    locomotion.grounded = grounded;
}

fn take_hits(
    mut hits: MessageReader<PlayerHit>,
    mut health: Single<&mut Health, With<Player>>,
    mut next: ResMut<NextState<GameState>>,
) {
    for hit in hits.read() {
        health.current -= hit.damage;
        health.since_hit = 0.0;
    }
    if health.current <= 0.0 {
        next.set(GameState::GameOver);
    }
}

fn regenerate(mut health: Single<&mut Health, With<Player>>, time: Res<Time>) {
    let dt = time.delta_secs();
    health.since_hit += dt;
    if health.since_hit >= HEALTH_REGEN_DELAY_SECS {
        let rate = health.max / HEALTH_REGEN_FULL_SECS;
        health.current = (health.current + rate * dt).min(health.max);
    }
}

impl Health {
    pub fn fraction(&self) -> f32 {
        (self.current / self.max).clamp(0.0, 1.0)
    }
}
