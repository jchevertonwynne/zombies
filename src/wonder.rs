//! The special Box Weapons that don't fire bullets: the Thundergun's blast, the Crossbow's bolt,
//! which sticks and explodes a moment later, and the Ballistic Knife's blade, which sticks where
//! it lands and can be picked back up. The Ray Gun fires through the ordinary splash path.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::ammo_mod::AmmoMods;
use crate::arsenal::{Splash, WeaponClass, WeaponId, WeaponStats};
use crate::equipment::Pull;
use crate::explosive::{Explosion, Explosive, Launch, Thrown, in_sight, makes_crawlers, world_filter};
use crate::layers::GameLayer;
use crate::player::Player;
use crate::state::{InRun, Pause};
use crate::tuning::*;
use crate::weapon::Loadout;
use crate::zombie::{Attack, DamageZombie, Dying, HitArea, HitZone, KnockDown, Zombie};

/// The held Weapon fired a Thundergun blast, a Crossbow bolt or a Ballistic Knife blade, sent in
/// a `Launch`.
#[derive(Clone, Copy)]
pub struct WonderFired {
    pub origin: Vec3,
    pub direction: Dir3,
    pub stats: &'static WeaponStats,
    pub upgraded: bool,
    /// Damage of a direct hit, already resolved through any modifiers.
    pub damage: f32,
    pub splash: Option<Splash>,
}

/// Whether a Weapon class fires through this module instead of as bullets.
pub fn is_wonder(class: WeaponClass) -> bool {
    matches!(class, WeaponClass::Thundergun | WeaponClass::Crossbow | WeaponClass::BallisticKnife)
}

/// What the Thundergun's blast does to a Zombie this far along its aim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blast {
    Kill,
    /// Knocked down for `THUNDERGUN_KNOCKDOWN_DAMAGE`.
    KnockDown,
}

/// The Thundergun's distance bands: killed under 12 m, knocked down to 30 m, and untouched
/// beyond. Its 12–23 m "gib" is only a look: the Thundergun never makes Crawlers.
pub fn thundergun_blast(distance: f32) -> Option<Blast> {
    if distance < THUNDERGUN_KILL_RANGE {
        Some(Blast::Kill)
    } else if distance <= THUNDERGUN_RANGE {
        Some(Blast::KnockDown)
    } else {
        None
    }
}

/// How far along the Thundergun's aim a point lies, if it's inside the blast's cylinder: in
/// front of the muzzle, within its length and its radius of the line.
pub fn thundergun_reach(origin: Vec3, aim: Dir3, point: Vec3) -> Option<f32> {
    let offset = point - origin;
    let along = offset.dot(*aim);
    let off_line = (offset - *aim * along).length();
    ((0.0..=THUNDERGUN_RANGE).contains(&along) && off_line <= THUNDERGUN_RADIUS).then_some(along)
}

/// Something stuck where it landed: in a Zombie, whom it follows (`offset` in its local space),
/// or in the world. A Zombie gone from under it leaves it to drop to the floor.
#[derive(Component)]
struct Stuck {
    host: Option<(Entity, Vec3)>,
}

/// A Crossbow bolt, burning down its fuse. The Awful Lawton's pulls Zombies meanwhile.
#[derive(Component)]
struct Bolt {
    /// The Player who fired it.
    player: Entity,
    fuse: f32,
    splash: Splash,
}

/// A Ballistic Knife blade, waiting to be picked back up.
#[derive(Component)]
struct Blade;

#[derive(Resource)]
struct WonderAssets {
    bolt: Handle<Mesh>,
    bolt_mat: Handle<StandardMaterial>,
    blade: Handle<Mesh>,
    blade_mat: Handle<StandardMaterial>,
}

pub struct WonderPlugin;

impl Plugin for WonderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_assets)
            .add_systems(
                Update,
                (fire_wonders, follow_hosts, burn_bolts, pick_up_blades)
                    .chain()
                    .run_if(in_state(Pause::Running)),
            );
    }
}

fn load_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(WonderAssets {
        bolt: meshes.add(Capsule3d::new(0.025, 0.4)),
        bolt_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.6, 0.15, 0.1),
            emissive: LinearRgba::rgb(3.0, 0.4, 0.2),
            ..default()
        }),
        blade: meshes.add(Cuboid::new(0.03, 0.25, 0.01)),
        blade_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.8, 0.8, 0.85),
            metallic: 0.9,
            perceptual_roughness: 0.3,
            ..default()
        }),
    });
}

/// The Thundergun blasts everything in its cylinder (the Sapper is only ever knocked down, which
/// stuns it); the bolt and the blade fly as hitscan, hit what they meet and stick there.
fn fire_wonders(
    mut commands: Commands,
    mut launches: MessageReader<Launch>,
    spatial: SpatialQuery,
    hit_areas: Query<(&HitArea, &ChildOf)>,
    zombies: Query<(Entity, &GlobalTransform, &Zombie), Without<Dying>>,
    mut damage: MessageWriter<DamageZombie>,
    mut knock_downs: MessageWriter<KnockDown>,
    assets: Res<WonderAssets>,
) {
    for launch in launches.read() {
        let Thrown::Wonder(shot) = launch.thrown else {
            continue;
        };
        let player = launch.player;
        let filter = SpatialQueryFilter::from_mask([GameLayer::World, GameLayer::ZombieHitArea])
            .with_excluded_entities([player]);
        if shot.stats.class == WeaponClass::Thundergun {
            for (zombie, transform, body) in &zombies {
                let centre = transform.translation();
                let Some(mut blast) = thundergun_reach(shot.origin, shot.direction, centre).and_then(thundergun_blast)
                else {
                    continue;
                };
                if body.is_sapper() {
                    blast = Blast::KnockDown;
                }
                if !in_sight(&spatial, shot.origin, centre) {
                    continue;
                }
                let amount = match blast {
                    Blast::Kill => f32::MAX,
                    Blast::KnockDown => {
                        // Flung straight away from the Player.
                        let away = (centre - shot.origin).with_y(0.0).normalize_or(shot.direction.with_y(0.0));
                        knock_downs.write(KnockDown {
                            zombie,
                            push: away * THUNDERGUN_FLING_SPEED,
                        });
                        THUNDERGUN_KNOCKDOWN_DAMAGE
                    }
                };
                damage.write(DamageZombie {
                    player: Some(player),
                    zombie,
                    damage: amount,
                    zone: HitZone::Torso,
                    attack: Attack::Bullet,
                });
            }
            continue;
        }

        // The bolt or blade: whatever its line meets first.
        let Some(hit) = spatial.cast_ray(shot.origin, shot.direction, HITSCAN_RANGE, true, &filter) else {
            continue;
        };
        // Just short of the surface, so a bolt's blast isn't blocked by what it's stuck in.
        let at = shot.origin + *shot.direction * (hit.distance - 0.1).max(0.0);
        let mut host = None;
        if let Ok((area, parent)) = hit_areas.get(hit.entity) {
            let zombie = parent.parent();
            damage.write(DamageZombie {
                player: Some(player),
                zombie,
                damage: shot.damage * shot.stats.zone_multiplier(area.0),
                zone: area.0,
                attack: Attack::Bullet,
            });
            if let Ok((_, transform, _)) = zombies.get(zombie) {
                host = Some((zombie, transform.affine().inverse().transform_point3(at)));
            }
        }
        let transform = Transform::from_translation(at).with_rotation(Quat::from_rotation_arc(Vec3::Y, *shot.direction));
        let stuck = (InRun, Stuck { host }, transform);
        match shot.stats.class {
            WeaponClass::Crossbow => {
                let fuse = if shot.upgraded { AWFUL_LAWTON_FUSE_SECS } else { CROSSBOW_FUSE_SECS };
                let Some(splash) = shot.splash else { continue };
                let mut bolt = commands.spawn((
                    stuck,
                    Bolt { player, fuse, splash },
                    Mesh3d(assets.bolt.clone()),
                    MeshMaterial3d(assets.bolt_mat.clone()),
                ));
                if shot.upgraded {
                    bolt.insert(Pull { at, left: fuse });
                }
            }
            _ => {
                commands.spawn((
                    stuck,
                    Blade,
                    Mesh3d(assets.blade.clone()),
                    MeshMaterial3d(assets.blade_mat.clone()),
                ));
            }
        }
    }
}

/// Stuck bolts and blades ride along in their Zombie, and drop to the floor once it's gone.
fn follow_hosts(
    mut stuck: Query<(&mut Stuck, &mut Transform)>,
    zombies: Query<&GlobalTransform, With<Zombie>>,
    spatial: SpatialQuery,
) {
    for (mut stuck, mut transform) in &mut stuck {
        let Some((host, offset)) = stuck.host else { continue };
        match zombies.get(host) {
            Ok(zombie) => transform.translation = zombie.transform_point(offset),
            Err(_) => {
                stuck.host = None;
                if let Some(floor) = spatial.cast_ray(transform.translation, Dir3::NEG_Y, 10.0, true, &world_filter()) {
                    transform.translation.y -= floor.distance - 0.05;
                }
            }
        }
    }
}

/// A bolt explodes when its fuse runs out. An Awful Lawton bolt pulls Zombies to the floor
/// below it until then.
fn burn_bolts(
    mut commands: Commands,
    mut bolts: Query<(Entity, &mut Bolt, &Transform, Option<&mut Pull>)>,
    spatial: SpatialQuery,
    mut explosions: MessageWriter<Explosion>,
    time: Res<Time>,
) {
    for (entity, mut bolt, transform, pull) in &mut bolts {
        bolt.fuse -= time.delta_secs();
        if bolt.fuse <= 0.0 {
            explosions.write(Explosion {
                at: transform.translation,
                thrower: Some(bolt.player),
                splash: bolt.splash,
                hurts_player: true,
                ammo_mods: AmmoMods::NONE,
                // Only the Crossbow and its Awful Lawton fire bolts.
                crawlers: makes_crawlers(Explosive::Weapon(WeaponId::Crossbow)),
            });
            commands.entity(entity).despawn();
            continue;
        }
        if let Some(mut pull) = pull {
            let floor = spatial
                .cast_ray(transform.translation, Dir3::NEG_Y, 10.0, true, &world_filter())
                .map_or(0.0, |hit| hit.distance);
            pull.at = transform.translation - Vec3::Y * floor;
            pull.left = bolt.fuse;
        }
    }
}

/// Walking over a blade picks it back up, as one more bullet for the Ballistic Knife, while the
/// Player carries it and has room. A blade still in a Zombie waits until it drops.
fn pick_up_blades(
    mut commands: Commands,
    blades: Query<(Entity, &Transform, &Stuck), With<Blade>>,
    player: Single<(&Transform, &mut Loadout), With<Player>>,
) {
    let (player_transform, mut loadout) = player.into_inner();
    for (entity, transform, stuck) in &blades {
        if stuck.host.is_none()
            && transform.translation.distance(player_transform.translation) <= BALLISTIC_KNIFE_PICKUP_RANGE
            && loadout.pick_up_bullet(WeaponId::BallisticKnife)
        {
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_thundergun_kills_under_12_m() {
        assert_eq!(thundergun_blast(0.0), Some(Blast::Kill));
        assert_eq!(thundergun_blast(11.9), Some(Blast::Kill));
    }

    #[test]
    fn the_thundergun_knocks_down_from_12_to_30_m() {
        for distance in [12.0, 17.5, 23.0, 27.0, 30.0] {
            assert_eq!(thundergun_blast(distance), Some(Blast::KnockDown), "{distance}");
        }
        assert_eq!(thundergun_blast(30.1), None);
    }

    #[test]
    fn the_thundergun_hits_inside_a_30_m_by_4_6_m_cylinder() {
        let (origin, aim) = (Vec3::ZERO, Dir3::NEG_Z);
        assert_eq!(thundergun_reach(origin, aim, Vec3::new(0.0, 0.0, -10.0)), Some(10.0));
        // Off the line, up to its radius.
        assert_eq!(thundergun_reach(origin, aim, Vec3::new(4.5, 0.0, -20.0)), Some(20.0));
        assert_eq!(thundergun_reach(origin, aim, Vec3::new(0.0, 4.7, -20.0)), None);
        // Not behind the muzzle, nor past the end.
        assert_eq!(thundergun_reach(origin, aim, Vec3::new(0.0, 0.0, 1.0)), None);
        assert_eq!(thundergun_reach(origin, aim, Vec3::new(0.0, 0.0, -30.5)), None);
    }

    #[test]
    fn only_the_thundergun_crossbow_and_ballistic_knife_skip_the_bullet_path() {
        let wonders: Vec<_> = crate::tuning::WEAPONS
            .iter()
            .filter(|row| is_wonder(row.class))
            .map(|row| row.id)
            .collect();
        assert_eq!(wonders, [WeaponId::Thundergun, WeaponId::Crossbow, WeaponId::BallisticKnife]);
    }
}
