//! Nova Crawlers: after Power, a few normal spawns drop from Vents in four indoor Zones (or rise
//! in the Yard) as low, quick crawling Zombies. Killing one with a bullet bursts a gas cloud that
//! blurs the Player's view, Shellshocks them up close, and kills the Zombies around it, so the
//! Knife is the clean way to deal with one.
//!
//! The Round decides when one spawns (see `round.rs`); this module holds their rules, their
//! bodies, and the gas.

use std::f32::consts::FRAC_PI_2;

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::hud::overlay::{Overlay, Overlays};
use crate::layers::GameLayer;
use crate::map::SpawnKind;
use crate::nav::Route;
use crate::player::Player;
use crate::round::zombie_health;
use crate::shellshock::Shellshock;
use crate::state::{InRun, Pause};
use crate::tuning::*;
use crate::zombie::{Attack, DamageZombie, Dying, HitArea, HitZone, Rising, Zombie};

/// Whether a normal spawn becomes a Nova Crawler: only after Power, never in a Hellhound Round,
/// with fewer than `NOVA_CRAWLER_MAX_ALIVE` alive, on a `roll` in `[0, 1)` under
/// `NOVA_CRAWLER_SPAWN_CHANCE`.
pub fn spawns_nova_crawler(power_on: bool, hellhound_round: bool, alive: u32, roll: f64) -> bool {
    power_on
        && !hellhound_round
        && alive < NOVA_CRAWLER_MAX_ALIVE
        && roll < NOVA_CRAWLER_SPAWN_CHANCE
}

/// A Nova Crawler's health in Round `round`: a share of a normal Zombie's.
pub fn nova_crawler_health(round: u32) -> f32 {
    zombie_health(round) as f32 * NOVA_CRAWLER_HEALTH_MULT
}

/// Where a Nova Crawler can come in: a Vent, or a Riser in the Yard. Never a window.
pub fn nova_crawler_entry(kind: SpawnKind) -> bool {
    matches!(kind, SpawnKind::Vent | SpawnKind::Riser)
}

/// Whether a Nova Crawler killed by `attack` bursts: only on a bullet killing blow (explosives
/// count, as their damage is a bullet's). A Knife kill, or another Nova Crawler's blast, never
/// does, so blasts never chain.
pub fn bursts(attack: Attack) -> bool {
    attack == Attack::Bullet
}

/// The Zombies a burst at `at` kills: every one within `NOVA_BLAST_RADIUS`, bar the Nova Crawler
/// that burst.
pub fn blast_victims(
    at: Vec3,
    burst: Entity,
    zombies: impl IntoIterator<Item = (Entity, Vec3)>,
) -> Vec<Entity> {
    zombies
        .into_iter()
        .filter(|&(zombie, centre)| zombie != burst && centre.distance(at) <= NOVA_BLAST_RADIUS)
        .map(|(zombie, _)| zombie)
        .collect()
}

/// Whether the Player, `distance` from a burst, is Shellshocked by it.
pub fn shellshocks(distance: f32) -> bool {
    distance <= NOVA_BLAST_RADIUS
}

/// A Nova Crawler died, from its centre at `at`. Sent beside its `ZombieKilled`.
#[derive(Message, Debug, Clone, Copy)]
pub struct NovaCrawlerKilled {
    pub zombie: Entity,
    pub at: Vec3,
    pub attack: Attack,
}

/// A Nova Crawler's swipe landed on the Player.
#[derive(Message, Debug, Clone, Copy)]
pub struct NovaSwipe;

/// A Nova Crawler that came in by dropping from a Vent.
#[derive(Component)]
pub struct VentDrop;

/// A burst's gas cloud, blurring the Player's view while they stand in it.
#[derive(Component)]
pub struct GasCloud {
    left: f32,
}

#[derive(Resource)]
struct NovaAssets {
    body: Handle<Mesh>,
    head: Handle<Mesh>,
    arm: Handle<Mesh>,
    eye: Handle<Mesh>,
    gas: Handle<Mesh>,
    skin: Handle<StandardMaterial>,
    eyes: Handle<StandardMaterial>,
    cloud: Handle<StandardMaterial>,
}

/// Lets the Round spawn Nova Crawlers without knowing about the asset handles.
#[derive(bevy::ecs::system::SystemParam)]
pub struct NovaAssetsParam<'w>(Res<'w, NovaAssets>);

const BODY_LENGTH: f32 = 0.6;
const HEAD_RADIUS: f32 = 0.16;
const NECK_SIZE: Vec3 = Vec3::new(0.15, 0.15, 0.12);
const ARM_SIZE: Vec3 = Vec3::new(0.1, 0.1, 0.6);

pub struct NovaPlugin;

impl Plugin for NovaPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<NovaCrawlerKilled>()
            .add_message::<NovaSwipe>()
            .add_systems(Startup, load_assets)
            .add_systems(
                Update,
                (burst, linger).chain().run_if(in_state(Pause::Running)),
            );
    }
}

fn load_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(NovaAssets {
        // Lying along its facing (-Z).
        body: meshes.add(
            Mesh::from(Capsule3d::new(NOVA_CRAWLER_RADIUS * 0.9, BODY_LENGTH))
                .rotated_by(Quat::from_rotation_x(FRAC_PI_2)),
        ),
        head: meshes.add(Sphere::new(HEAD_RADIUS)),
        arm: meshes.add(Cuboid::from_size(ARM_SIZE)),
        eye: meshes.add(Sphere::new(0.03)),
        gas: meshes.add(Sphere::new(NOVA_GAS_RADIUS)),
        skin: materials.add(StandardMaterial {
            base_color: Color::srgb(0.55, 0.6, 0.35),
            emissive: LinearRgba::rgb(0.08, 0.12, 0.0),
            ..default()
        }),
        eyes: materials.add(StandardMaterial {
            base_color: Color::srgb(0.6, 1.0, 0.2),
            emissive: LinearRgba::rgb(4.0, 10.0, 1.0),
            ..default()
        }),
        cloud: materials.add(StandardMaterial {
            base_color: Color::srgba(0.75, 0.85, 0.35, 0.25),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            double_sided: true,
            cull_mode: None,
            ..default()
        }),
    });
}

/// Spawns a Nova Crawler coming in at a Vent or a Riser, `floor` being the spot it ends up on.
/// From a Vent it drops `NOVA_VENT_HEIGHT` over `NOVA_VENT_DROP_SECS`; at a Riser it climbs out
/// of the ground over `ZOMBIE_RISE_SECS`. Either way it can be shot meanwhile. It never gets
/// `Entering`: it never uses Barriers.
pub fn spawn_nova_crawler(
    commands: &mut Commands,
    assets: &NovaAssetsParam,
    kind: SpawnKind,
    floor: Vec3,
    health: f32,
) -> Entity {
    let assets = &assets.0;
    let standing = floor.y + NOVA_CRAWLER_RADIUS + 0.02;
    let (from, secs) = if kind == SpawnKind::Vent {
        (floor.y + NOVA_VENT_HEIGHT, NOVA_VENT_DROP_SECS)
    } else {
        (
            standing - 2.0 * NOVA_CRAWLER_RADIUS - 0.05,
            ZOMBIE_RISE_SECS,
        )
    };
    let centre = Vec3::new(floor.x, from, floor.z);
    let mut nova = commands.spawn((
        InRun,
        Name::new("Nova Crawler"),
        Zombie::nova_crawler(health, centre),
        Rising::new(from, standing, secs),
        Route::default(),
        Mesh3d(assets.body.clone()),
        MeshMaterial3d(assets.skin.clone()),
        Transform::from_translation(centre),
        RigidBody::Kinematic,
        CustomPositionIntegration,
        TranslationInterpolation,
        Collider::sphere(NOVA_CRAWLER_RADIUS),
        CollisionLayers::new(GameLayer::Zombie, LayerMask::ALL),
    ));
    if kind == SpawnKind::Vent {
        nova.insert(VentDrop);
    }
    nova.with_children(|nova| {
        let hit_layers = CollisionLayers::new(GameLayer::ZombieHitArea, LayerMask::ALL);
        let front = -(BODY_LENGTH / 2.0 + NOVA_CRAWLER_RADIUS);
        nova.spawn((
            HitArea(HitZone::Head),
            Mesh3d(assets.head.clone()),
            MeshMaterial3d(assets.skin.clone()),
            Transform::from_xyz(0.0, NOVA_CRAWLER_RADIUS - HEAD_RADIUS, front),
            Collider::sphere(HEAD_RADIUS),
            hit_layers,
        ))
        .with_children(|head| {
            for x in [-0.06, 0.06] {
                head.spawn((
                    Mesh3d(assets.eye.clone()),
                    MeshMaterial3d(assets.eyes.clone()),
                    Transform::from_xyz(x, 0.04, -HEAD_RADIUS * 0.9),
                ));
            }
        });
        nova.spawn((
            HitArea(HitZone::Neck),
            Transform::from_xyz(0.0, 0.0, front + HEAD_RADIUS + NECK_SIZE.z / 2.0),
            Collider::cuboid(NECK_SIZE.x, NECK_SIZE.y, NECK_SIZE.z),
            hit_layers,
        ));
        nova.spawn((
            HitArea(HitZone::Torso),
            Transform::from_xyz(0.0, 0.0, NECK_SIZE.z / 2.0),
            Collider::cuboid(
                NOVA_CRAWLER_RADIUS * 2.0,
                NOVA_CRAWLER_RADIUS * 2.0,
                BODY_LENGTH + NOVA_CRAWLER_RADIUS,
            ),
            hit_layers,
        ));
        // Long arms out in front, clawing it along.
        for x in [-NOVA_CRAWLER_RADIUS - 0.05, NOVA_CRAWLER_RADIUS + 0.05] {
            nova.spawn((
                HitArea(HitZone::Limb),
                Mesh3d(assets.arm.clone()),
                MeshMaterial3d(assets.skin.clone()),
                Transform::from_xyz(x, ARM_SIZE.y / 2.0 - NOVA_CRAWLER_RADIUS, front),
                Collider::cuboid(ARM_SIZE.x, ARM_SIZE.y, ARM_SIZE.z),
                hit_layers,
            ));
        }
    });
    nova.id()
}

/// A Nova Crawler killed by a bullet bursts: a gas cloud where it died, a Shellshock for a Player
/// close by, and an Unpaid kill of every Zombie around it.
fn burst(
    mut commands: Commands,
    mut killed: MessageReader<NovaCrawlerKilled>,
    zombies: Query<(Entity, &Transform), (With<Zombie>, Without<Dying>)>,
    player: Single<&Transform, With<Player>>,
    mut shocks: MessageWriter<Shellshock>,
    mut damage: MessageWriter<DamageZombie>,
    assets: Res<NovaAssets>,
) {
    for kill in killed.read() {
        if !bursts(kill.attack) {
            continue;
        }
        commands.spawn((
            InRun,
            Name::new("Nova gas"),
            GasCloud {
                left: NOVA_GAS_SECS,
            },
            Mesh3d(assets.gas.clone()),
            MeshMaterial3d(assets.cloud.clone()),
            Transform::from_translation(kill.at),
        ));
        if shellshocks(player.translation.distance(kill.at)) {
            shocks.write(Shellshock {
                secs: NOVA_SHELLSHOCK_SECS,
            });
        }
        let around = zombies
            .iter()
            .map(|(zombie, transform)| (zombie, transform.translation));
        for zombie in blast_victims(kill.at, kill.zombie, around) {
            damage.write(DamageZombie {
                zombie,
                damage: f32::INFINITY,
                zone: HitZone::Torso,
                attack: Attack::NovaBlast,
            });
        }
    }
}

/// A gas cloud blurs the Player's view while they stand in it, then clears.
fn linger(
    mut commands: Commands,
    mut clouds: Query<(Entity, &mut GasCloud, &Transform)>,
    player: Single<&Transform, With<Player>>,
    mut overlays: ResMut<Overlays>,
    time: Res<Time>,
) {
    for (entity, mut cloud, transform) in &mut clouds {
        cloud.left -= time.delta_secs();
        if cloud.left <= 0.0 {
            commands.entity(entity).despawn();
        } else if player.translation.distance(transform.translation) <= NOVA_GAS_RADIUS {
            overlays.hold(Overlay::Shellshock, NOVA_GAS_BLUR_STRENGTH);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::BarrierId;

    #[test]
    fn nova_crawlers_spawn_only_after_power() {
        assert!(!spawns_nova_crawler(false, false, 0, 0.0));
        assert!(spawns_nova_crawler(true, false, 0, 0.0));
    }

    #[test]
    fn a_tenth_of_normal_spawns_are_nova_crawlers() {
        let novas = (0..1000)
            .filter(|i| spawns_nova_crawler(true, false, 0, *i as f64 / 1000.0))
            .count();
        assert_eq!(novas, 100);
    }

    #[test]
    fn at_most_six_nova_crawlers_are_alive() {
        assert!(spawns_nova_crawler(true, false, 5, 0.0));
        assert!(!spawns_nova_crawler(true, false, 6, 0.0));
        assert!(!spawns_nova_crawler(true, false, 9, 0.0));
    }

    #[test]
    fn no_nova_crawlers_on_hellhound_rounds() {
        assert!(!spawns_nova_crawler(true, true, 0, 0.0));
    }

    #[test]
    fn nova_crawlers_come_from_vents_or_risers_never_windows() {
        assert!(nova_crawler_entry(SpawnKind::Vent));
        assert!(nova_crawler_entry(SpawnKind::Riser));
        assert!(!nova_crawler_entry(SpawnKind::Window(BarrierId(0))));
        assert!(!nova_crawler_entry(SpawnKind::Hellhound));
    }

    #[test]
    fn a_nova_crawler_has_three_quarters_of_a_zombies_health() {
        assert_eq!(nova_crawler_health(1), 112.5);
        assert_eq!(nova_crawler_health(10), 1045.0 * 0.75);
    }

    #[test]
    fn only_a_bullet_killing_blow_bursts_the_gas() {
        assert!(bursts(Attack::Bullet));
        assert!(!bursts(Attack::Knife), "the Knife is the clean kill");
        for attack in [Attack::Burn, Attack::Trap, Attack::Nuke, Attack::NovaBlast] {
            assert!(!bursts(attack), "{attack:?}");
        }
    }

    #[test]
    fn the_blast_kills_other_zombies_within_two_point_four_metres() {
        let mut world = World::new();
        let [burst, near, edge, far] = [(); 4].map(|_| world.spawn_empty().id());
        let zombies = [
            (burst, Vec3::ZERO),
            (near, Vec3::new(1.0, 0.0, 0.0)),
            (edge, Vec3::new(0.0, 0.0, 2.4)),
            (far, Vec3::new(2.5, 0.0, 0.0)),
        ];
        assert_eq!(blast_victims(Vec3::ZERO, burst, zombies), [near, edge]);
    }

    #[test]
    fn a_blast_kill_is_unpaid_and_never_chains() {
        assert_eq!(Attack::NovaBlast.pay(), crate::zombie::Pay::Unpaid);
        assert!(!bursts(Attack::NovaBlast));
    }

    #[test]
    fn the_player_is_shellshocked_only_up_close() {
        assert!(shellshocks(1.0));
        assert!(shellshocks(2.4));
        assert!(!shellshocks(2.5));
    }
}
