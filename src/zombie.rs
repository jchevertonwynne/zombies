use std::f32::consts::FRAC_PI_2;

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::layers::GameLayer;
use crate::player::{Player, PlayerHit};
use crate::state::{InRun, Pause};
use crate::tuning::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeedTier {
    Walker,
    Jogger,
    Sprinter,
}

impl SpeedTier {
    pub fn speed(self) -> f32 {
        match self {
            SpeedTier::Walker => ZOMBIE_WALK_SPEED,
            SpeedTier::Jogger => ZOMBIE_JOG_SPEED,
            SpeedTier::Sprinter => ZOMBIE_SPRINT_SPEED,
        }
    }
}

/// How a Zombie was hit, which decides damage and Points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitKind {
    Body,
    Head,
    Knife,
}

/// Request to damage a Zombie, written by the Player's attacks.
#[derive(Message)]
pub struct DamageZombie {
    pub zombie: Entity,
    pub damage: f32,
    pub kind: HitKind,
}

/// A Zombie took damage. `killed` is true for the hit that finished it.
#[derive(Message, Debug, Clone, Copy)]
pub struct ZombieHit {
    pub kind: HitKind,
    pub killed: bool,
}

#[derive(Component)]
pub struct Zombie {
    health: f32,
    tier: SpeedTier,
    attack_cooldown: f32,
    /// Seconds left before a swing lands; `None` when not swinging.
    wind_up: Option<f32>,
}

/// The head hitbox. Its parent is the Zombie.
#[derive(Component)]
pub struct ZombieHead;

/// A killed Zombie falling over before it disappears.
#[derive(Component, Default)]
pub struct Dying {
    elapsed: f32,
}

#[derive(Resource)]
struct ZombieAssets {
    body: Handle<Mesh>,
    head: Handle<Mesh>,
    arm: Handle<Mesh>,
    eye: Handle<Mesh>,
    skin: Handle<StandardMaterial>,
    clothes: Handle<StandardMaterial>,
    eyes: Handle<StandardMaterial>,
}

pub struct ZombiePlugin;

impl Plugin for ZombiePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<DamageZombie>()
            .add_message::<ZombieHit>()
            .add_systems(Startup, load_assets)
            .add_systems(FixedUpdate, chase.run_if(in_state(Pause::Running)))
            .add_systems(
                Update,
                (attack, apply_damage, fall_over).run_if(in_state(Pause::Running)),
            );
    }
}

fn load_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(ZombieAssets {
        body: meshes.add(Capsule3d::new(ZOMBIE_RADIUS, ZOMBIE_CAPSULE_LENGTH)),
        head: meshes.add(Sphere::new(ZOMBIE_HEAD_RADIUS)),
        arm: meshes.add(Cuboid::new(0.12, 0.12, 0.6)),
        eye: meshes.add(Sphere::new(0.035)),
        skin: materials.add(Color::srgb(0.45, 0.52, 0.38)),
        clothes: materials.add(Color::srgb(0.22, 0.24, 0.2)),
        eyes: materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.6, 0.1),
            emissive: LinearRgba::rgb(8.0, 4.0, 0.5),
            ..default()
        }),
    });
}

fn body_half_height() -> f32 {
    ZOMBIE_RADIUS + ZOMBIE_CAPSULE_LENGTH / 2.0
}

/// Spawns a Zombie standing on the floor at `at`.
pub fn spawn_zombie(commands: &mut Commands, assets: &ZombieAssetsParam, at: Vec3, health: f32, tier: SpeedTier) {
    let assets = &assets.0;
    let half = body_half_height();
    let head_offset = half + ZOMBIE_HEAD_RADIUS * 0.5;
    // Hover just above the floor so horizontal movement never scrapes it.
    let centre = Vec3::new(at.x, half + 0.02, at.z);

    commands
        .spawn((
            InRun,
            Name::new("Zombie"),
            Zombie {
                health,
                tier,
                attack_cooldown: 0.0,
                wind_up: None,
            },
            Mesh3d(assets.body.clone()),
            MeshMaterial3d(assets.clothes.clone()),
            Transform::from_translation(centre),
            RigidBody::Kinematic,
            CustomPositionIntegration,
            TranslationInterpolation,
            Collider::capsule(ZOMBIE_RADIUS, ZOMBIE_CAPSULE_LENGTH),
            CollisionLayers::new(GameLayer::Zombie, LayerMask::ALL),
        ))
        .with_children(|zombie| {
            zombie
                .spawn((
                    ZombieHead,
                    Mesh3d(assets.head.clone()),
                    MeshMaterial3d(assets.skin.clone()),
                    Transform::from_xyz(0.0, head_offset, 0.0),
                    Collider::sphere(ZOMBIE_HEAD_RADIUS),
                    CollisionLayers::new(GameLayer::ZombieHead, LayerMask::ALL),
                ))
                .with_children(|head| {
                    for x in [-0.07, 0.07] {
                        head.spawn((
                            Mesh3d(assets.eye.clone()),
                            MeshMaterial3d(assets.eyes.clone()),
                            Transform::from_xyz(x, 0.04, -ZOMBIE_HEAD_RADIUS * 0.9),
                        ));
                    }
                });
            // Arms reaching forward (-Z is forward).
            for x in [-ZOMBIE_RADIUS - 0.05, ZOMBIE_RADIUS + 0.05] {
                zombie.spawn((
                    Mesh3d(assets.arm.clone()),
                    MeshMaterial3d(assets.skin.clone()),
                    Transform::from_xyz(x, half * 0.55, -0.3),
                ));
            }
        });
}

/// Lets other modules spawn Zombies without knowing about the asset handles.
#[derive(bevy::ecs::system::SystemParam)]
pub struct ZombieAssetsParam<'w>(Res<'w, ZombieAssets>);

fn horizontal(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

fn chase(
    mut zombies: Query<(Entity, &Zombie, &mut Transform, &Collider), Without<Dying>>,
    player: Single<&Transform, (With<Player>, Without<Zombie>)>,
    move_and_slide: MoveAndSlide,
    time: Res<Time>,
) {
    let target = player.translation;
    let filter_mask = [GameLayer::World, GameLayer::Zombie, GameLayer::Player];

    for (entity, zombie, mut transform, collider) in &mut zombies {
        let to_player = horizontal(target - transform.translation);
        if to_player.length_squared() > 1e-4 {
            transform.rotation = Quat::from_rotation_y(f32::atan2(-to_player.x, -to_player.z));
        }

        // Plant feet while swinging, so the Player can back out of reach.
        if zombie.wind_up.is_some() || to_player.length() <= ZOMBIE_REACH * 0.8 {
            continue;
        }

        let velocity = to_player.normalize_or_zero() * zombie.tier.speed();
        let out = move_and_slide.move_and_slide(
            collider,
            transform.translation,
            transform.rotation,
            velocity,
            time.delta(),
            &MoveAndSlideConfig::default(),
            &SpatialQueryFilter::from_mask(filter_mask).with_excluded_entities([entity]),
            |_| MoveAndSlideHitResponse::Accept,
        );
        transform.translation = Vec3::new(out.position.x, transform.translation.y, out.position.z);
    }
}

fn attack(
    mut zombies: Query<(&mut Zombie, &Transform), Without<Dying>>,
    player: Single<&Transform, With<Player>>,
    mut hits: MessageWriter<PlayerHit>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    for (mut zombie, transform) in &mut zombies {
        let in_reach = horizontal(player.translation - transform.translation).length() <= ZOMBIE_REACH;
        zombie.attack_cooldown = (zombie.attack_cooldown - dt).max(0.0);

        match zombie.wind_up {
            Some(left) if left - dt <= 0.0 => {
                zombie.wind_up = None;
                zombie.attack_cooldown = ZOMBIE_ATTACK_COOLDOWN_SECS;
                if in_reach {
                    hits.write(PlayerHit { damage: ZOMBIE_DAMAGE });
                }
            }
            Some(left) => zombie.wind_up = Some(left - dt),
            None if in_reach && zombie.attack_cooldown <= 0.0 => {
                zombie.wind_up = Some(ZOMBIE_WIND_UP_SECS);
            }
            None => {}
        }
    }
}

fn apply_damage(
    mut commands: Commands,
    mut requests: MessageReader<DamageZombie>,
    mut zombies: Query<(&mut Zombie, &Children), Without<Dying>>,
    heads: Query<(), With<ZombieHead>>,
    mut hits: MessageWriter<ZombieHit>,
) {
    for request in requests.read() {
        let Ok((mut zombie, children)) = zombies.get_mut(request.zombie) else {
            continue;
        };
        // Several hits can land in one frame; only the first lethal one counts.
        if zombie.health <= 0.0 {
            continue;
        }
        zombie.health -= request.damage;
        let killed = zombie.health <= 0.0;
        hits.write(ZombieHit {
            kind: request.kind,
            killed,
        });

        if killed {
            commands
                .entity(request.zombie)
                .insert(Dying::default())
                .remove::<(Collider, CollisionLayers)>();
            for child in children.iter() {
                if heads.contains(child) {
                    commands.entity(child).remove::<(Collider, CollisionLayers)>();
                }
            }
        }
    }
}

fn fall_over(
    mut commands: Commands,
    mut dying: Query<(Entity, &mut Dying, &mut Transform)>,
    time: Res<Time>,
) {
    for (entity, mut dying, mut transform) in &mut dying {
        let before = (dying.elapsed / ZOMBIE_FALL_SECS).min(1.0);
        dying.elapsed += time.delta_secs();
        let after = (dying.elapsed / ZOMBIE_FALL_SECS).min(1.0);

        // Tip backwards about the local X axis, sinking so it lies on the floor.
        let step = (after - before) * FRAC_PI_2;
        if step > 0.0 {
            transform.rotate_local_x(step);
            transform.translation.y -= (after - before) * (body_half_height() - ZOMBIE_RADIUS);
        }

        if dying.elapsed >= ZOMBIE_FALL_SECS + ZOMBIE_CORPSE_SECS {
            commands.entity(entity).despawn();
        }
    }
}
