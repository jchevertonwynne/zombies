//! Explosions and the projectiles that carry them: a blast hurts every Zombie in sight within its
//! radius, falling off from centre to edge, and the Player's own explosives cost them a flat 75
//! while they're above 75 Health.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::ammo_mod::{AmmoModHit, AmmoMods};
use crate::arsenal::{Splash, WeaponId};
use crate::layers::GameLayer;
use crate::player::{Health, Player, PlayerHit};
use crate::settings::Settings;
use crate::state::{InRun, Pause};
use crate::tuning::*;
use crate::zombie::{Attack, DamageZombie, Dying, Gibbed, HitArea, HitZone, Zombie, blows_legs_off};

/// A blast going off. Every explosive, from a frag to a launcher's rocket, sends one.
#[derive(Message, Debug, Clone, Copy)]
pub struct Explosion {
    pub at: Vec3,
    /// Damage from centre to edge, already resolved through any modifiers.
    pub splash: Splash,
    /// Whether the blast hurts the Player. Their own frags, launchers and Monkey Bombs do;
    /// Claymores never do.
    pub hurts_player: bool,
    /// The Ammo Mods of the Weapon that fired it, which each Zombie it hurts rolls. None for
    /// Equipment.
    pub ammo_mods: AmmoMods,
    /// Whether the blast can blow a Zombie's legs off, making a Crawler. Classic explosives set
    /// it from `makes_crawlers`; anything else that may (Scorched Earth) just sends it set.
    pub crawlers: bool,
}

/// Where a blast came from, for the Crawler rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Explosive {
    Frag,
    Claymore,
    MonkeyBomb,
    GrenadeLauncher,
    /// A Weapon's own splash: a launcher's, the Ray Gun's, a Crossbow bolt's, Mustang & Sally's…
    /// An Upgraded Weapon keeps its base's id, so the Awful Lawton is the Crossbow here.
    Weapon(WeaponId),
}

/// The set of explosives that can make Crawlers: frags, Claymores, the Monkey Bomb, launchers,
/// the Grenade Launcher, Ray Gun splash, and the Crossbow and its Awful Lawton. Everything else
/// never does.
pub fn makes_crawlers(explosive: Explosive) -> bool {
    match explosive {
        Explosive::Frag | Explosive::Claymore | Explosive::MonkeyBomb | Explosive::GrenadeLauncher => true,
        Explosive::Weapon(id) => {
            matches!(id, WeaponId::M72Law | WeaponId::ChinaLake | WeaponId::RayGun | WeaponId::Crossbow)
        }
    }
}

/// What the Player takes from their own explosive: a flat `SELF_EXPLOSIVE_DAMAGE` while their
/// Health is above it (never more than the blast itself does), and the full damage at or below it.
pub fn self_explosive_damage(health: f32, damage: f32) -> f32 {
    if health > SELF_EXPLOSIVE_DAMAGE {
        damage.min(SELF_EXPLOSIVE_DAMAGE)
    } else {
        damage
    }
}

/// How a launcher's shot flies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flight {
    pub speed: f32,
    /// Downward acceleration, 0 for a rocket that flies straight.
    pub gravity: f32,
}

/// The real projectile a Weapon fires, if it fires one instead of a hitscan bullet.
pub fn flight(id: WeaponId) -> Option<Flight> {
    match id {
        WeaponId::M72Law => Some(Flight {
            speed: LAW_ROCKET_SPEED,
            gravity: 0.0,
        }),
        WeaponId::ChinaLake => Some(Flight {
            speed: CHINA_LAKE_GRENADE_SPEED,
            gravity: GRAVITY,
        }),
        _ => None,
    }
}

/// A launcher fired: its projectile leaves from `origin`.
#[derive(Message, Debug, Clone, Copy)]
pub struct Launched {
    pub origin: Vec3,
    pub direction: Dir3,
    pub flight: Flight,
    /// Damage to a Zombie it hits directly, before the hit zone.
    pub impact: f32,
    pub impact_headshot: f32,
    /// Whether a direct hit to the head counts as a Headshot. The Grenade Launcher's never does.
    pub headshots: bool,
    pub splash: Splash,
    /// The launcher's Ammo Mods, rolled by its impact and its blast.
    pub ammo_mods: AmmoMods,
    /// Whether its blast can make Crawlers.
    pub crawlers: bool,
}

/// A launcher's projectile in flight. It bursts on the first thing it touches.
#[derive(Component)]
struct Projectile {
    velocity: Vec3,
    gravity: f32,
    impact: f32,
    impact_headshot: f32,
    headshots: bool,
    splash: Splash,
    ammo_mods: AmmoMods,
    crawlers: bool,
    life: f32,
}

/// The fireball an explosion leaves for a moment.
#[derive(Component)]
struct Fireball {
    age: f32,
    radius: f32,
}

const FIREBALL_SECS: f32 = 0.3;

#[derive(Resource)]
struct ExplosiveAssets {
    ball: Handle<Mesh>,
    fire: Handle<StandardMaterial>,
    projectile: Handle<Mesh>,
    casing: Handle<StandardMaterial>,
}

pub struct ExplosivePlugin;

impl Plugin for ExplosivePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Explosion>()
            .add_message::<Launched>()
            .add_systems(Startup, load_assets)
            .add_systems(
                Update,
                (launch, fly, explode, fade_fireballs)
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
    commands.insert_resource(ExplosiveAssets {
        ball: meshes.add(Sphere::new(1.0)),
        fire: materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.6, 0.2, 0.5),
            emissive: LinearRgba::rgb(20.0, 8.0, 1.0),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        }),
        projectile: meshes.add(Capsule3d::new(0.05, 0.2)),
        casing: materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 0.32, 0.25),
            emissive: LinearRgba::rgb(2.0, 1.0, 0.2),
            ..default()
        }),
    });
}

/// Explosions are blocked by the world, not by Zombies or anything thrown.
pub fn world_filter() -> SpatialQueryFilter {
    SpatialQueryFilter::from_mask(GameLayer::World)
}

/// Whether nothing solid stands between two points.
pub fn in_sight(spatial: &SpatialQuery, from: Vec3, to: Vec3) -> bool {
    let Ok((direction, distance)) = Dir3::new_and_length(to - from) else {
        return true;
    };
    spatial
        .cast_ray(from, direction, distance, true, &world_filter())
        .is_none_or(|hit| hit.distance >= distance - 0.05)
}

fn launch(mut commands: Commands, mut launched: MessageReader<Launched>, assets: Res<ExplosiveAssets>) {
    for shot in launched.read() {
        commands.spawn((
            InRun,
            Projectile {
                velocity: *shot.direction * shot.flight.speed,
                gravity: shot.flight.gravity,
                impact: shot.impact,
                impact_headshot: shot.impact_headshot,
                headshots: shot.headshots,
                splash: shot.splash,
                ammo_mods: shot.ammo_mods,
                crawlers: shot.crawlers,
                life: PROJECTILE_LIFETIME_SECS,
            },
            Mesh3d(assets.projectile.clone()),
            MeshMaterial3d(assets.casing.clone()),
            Transform::from_translation(shot.origin).with_rotation(Quat::from_rotation_arc(Vec3::Y, *shot.direction)),
        ));
    }
}

/// Moves each projectile along its path, bursting on the world or a Zombie's hit area.
fn fly(
    mut commands: Commands,
    mut projectiles: Query<(Entity, &mut Projectile, &mut Transform)>,
    spatial: SpatialQuery,
    hit_areas: Query<(&HitArea, &ChildOf)>,
    mut damage: MessageWriter<DamageZombie>,
    mut ammo_mod_hits: MessageWriter<AmmoModHit>,
    mut explosions: MessageWriter<Explosion>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    let filter = SpatialQueryFilter::from_mask([GameLayer::World, GameLayer::ZombieHitArea]);
    for (entity, mut projectile, mut transform) in &mut projectiles {
        projectile.life -= dt;
        let step = projectile.velocity * dt;
        let burst_at = match Dir3::new_and_length(step) {
            Ok((direction, length)) => spatial
                .cast_ray(transform.translation, direction, length, true, &filter)
                .map(|hit| {
                    if let Ok((area, parent)) = hit_areas.get(hit.entity) {
                        let multiplier = if area.0 == HitZone::Head { projectile.impact_headshot } else { 1.0 };
                        let (zombie, amount) = (parent.parent(), projectile.impact * multiplier);
                        damage.write(DamageZombie {
                            zombie,
                            damage: amount,
                            zone: if projectile.headshots { area.0 } else { HitZone::Torso },
                            attack: Attack::Bullet,
                        });
                        if !projectile.ammo_mods.is_empty() {
                            ammo_mod_hits.write(AmmoModHit {
                                zombie,
                                damage: amount,
                                ammo_mods: projectile.ammo_mods,
                            });
                        }
                    }
                    // Burst just short of the surface, so the world doesn't block its own blast.
                    transform.translation + *direction * (hit.distance - 0.1).max(0.0)
                }),
            Err(_) => None,
        };
        let burst_at = burst_at.or((projectile.life <= 0.0).then_some(transform.translation));
        if let Some(at) = burst_at {
            explosions.write(Explosion {
                at,
                splash: projectile.splash,
                hurts_player: true,
                ammo_mods: projectile.ammo_mods,
                crawlers: projectile.crawlers,
            });
            commands.entity(entity).despawn();
            continue;
        }
        transform.translation += step;
        let gravity = projectile.gravity;
        projectile.velocity.y -= gravity * dt;
        if let Ok(direction) = Dir3::new(projectile.velocity) {
            transform.rotation = Quat::from_rotation_arc(Vec3::Y, *direction);
        }
    }
}

/// Every blast hurts the Zombies in sight within its radius, and the Player if it's theirs. One
/// that can make Crawlers may blow the legs off a Zombie it doesn't kill, while Crawlers are
/// switched on in Settings.
fn explode(
    mut commands: Commands,
    mut explosions: MessageReader<Explosion>,
    spatial: SpatialQuery,
    zombies: Query<(Entity, &Zombie, &GlobalTransform), Without<Dying>>,
    players: Query<(Entity, &Transform, &Health), With<Player>>,
    mut damage: MessageWriter<DamageZombie>,
    mut ammo_mod_hits: MessageWriter<AmmoModHit>,
    mut gibbed: MessageWriter<Gibbed>,
    mut player_hits: MessageWriter<PlayerHit>,
    assets: Res<ExplosiveAssets>,
    settings: Res<Settings>,
) {
    let crawlers_on = settings.special_zombies.crawlers;
    // Several blasts in one frame each see the Health the last one left.
    let mut health_left: Vec<_> = players.iter().map(|(player, _, health)| (player, health.current)).collect();
    // Likewise each Zombie's, and whether it can't lose its legs: already a Crawler, or a
    // Special Zombie, which is never a normal Zombie.
    let mut blasted: Vec<(Entity, f32, bool)> = Vec::new();
    for explosion in explosions.read() {
        let splash = explosion.splash;
        for (zombie, body, transform) in &zombies {
            let centre = transform.translation();
            let Some(amount) = splash.damage(centre.distance(explosion.at)) else {
                continue;
            };
            if !in_sight(&spatial, explosion.at, centre) {
                continue;
            }
            damage.write(DamageZombie {
                zombie,
                damage: amount,
                zone: HitZone::Torso,
                attack: Attack::Bullet,
            });
            if !explosion.ammo_mods.is_empty() {
                ammo_mod_hits.write(AmmoModHit {
                    zombie,
                    damage: amount,
                    ammo_mods: explosion.ammo_mods,
                });
            }
            let i = blasted.iter().position(|&(e, ..)| e == zombie).unwrap_or_else(|| {
                blasted.push((zombie, body.health(), !body.can_lose_legs()));
                blasted.len() - 1
            });
            let (_, before, not_normal) = blasted[i];
            if explosion.crawlers && blows_legs_off(crawlers_on, !not_normal, before, amount, rand::random()) {
                gibbed.write(Gibbed { zombie });
                blasted[i].2 = true;
            }
            blasted[i].1 = before - amount;
        }

        // It doesn't yet say whose it is, so a blast that hurts the Player hurts every Player it reaches.
        if explosion.hurts_player {
            for (player, transform, _) in &players {
                let centre = transform.translation;
                if let Some(amount) = splash.damage(centre.distance(explosion.at))
                    && in_sight(&spatial, explosion.at, centre)
                {
                    let left = &mut health_left.iter_mut().find(|(entity, _)| *entity == player).expect("listed above").1;
                    let taken = self_explosive_damage(*left, amount);
                    *left -= taken;
                    player_hits.write(PlayerHit { player, damage: taken });
                }
            }
        }

        commands.spawn((
            InRun,
            Fireball {
                age: 0.0,
                radius: splash.radius,
            },
            Mesh3d(assets.ball.clone()),
            MeshMaterial3d(assets.fire.clone()),
            Transform::from_translation(explosion.at).with_scale(Vec3::splat(0.01)),
        ));
    }
}

fn fade_fireballs(mut commands: Commands, mut fireballs: Query<(Entity, &mut Fireball, &mut Transform)>, time: Res<Time>) {
    for (entity, mut fireball, mut transform) in &mut fireballs {
        fireball.age += time.delta_secs();
        if fireball.age >= FIREBALL_SECS {
            commands.entity(entity).despawn();
        } else {
            // Swell to half the blast radius and shrink away.
            let t = fireball.age / FIREBALL_SECS;
            let size = fireball.radius * 0.5 * (t * std::f32::consts::PI).sin().max(0.05);
            transform.scale = Vec3::splat(size);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn own_explosives_cost_a_flat_75_above_75_health() {
        assert_eq!(self_explosive_damage(100.0, 300.0), 75.0);
        assert_eq!(self_explosive_damage(250.0, 5000.0), 75.0);
        assert_eq!(self_explosive_damage(75.1, 300.0), 75.0);
    }

    #[test]
    fn own_explosives_do_full_damage_at_or_below_75_health() {
        assert_eq!(self_explosive_damage(75.0, 300.0), 300.0);
        assert_eq!(self_explosive_damage(40.0, 120.0), 120.0);
    }

    #[test]
    fn the_flat_cost_never_exceeds_the_blast() {
        assert_eq!(self_explosive_damage(100.0, 50.0), 50.0);
    }

    #[test]
    fn a_frag_falls_off_from_300_to_75() {
        assert_eq!(FRAG_SPLASH.damage(0.0), Some(300.0));
        assert!(close(FRAG_SPLASH.damage(FRAG_SPLASH.radius).unwrap(), 75.0));
        assert!(close(FRAG_SPLASH.damage(FRAG_SPLASH.radius / 2.0).unwrap(), 187.5));
        assert_eq!(FRAG_SPLASH.damage(FRAG_SPLASH.radius + 0.01), None);
    }

    #[test]
    fn a_claymore_falls_off_from_200_to_50_within_5_m() {
        assert_eq!(CLAYMORE_SPLASH.damage(0.0), Some(200.0));
        assert!(close(CLAYMORE_SPLASH.damage(2.5).unwrap(), 125.0));
        assert!(close(CLAYMORE_SPLASH.damage(5.0).unwrap(), 50.0));
        assert_eq!(CLAYMORE_SPLASH.damage(5.1), None);
    }

    #[test]
    fn a_monkey_bomb_falls_off_from_5000_to_450_within_2_5_m() {
        assert_eq!(MONKEY_BOMB_SPLASH.damage(0.0), Some(5000.0));
        assert!(close(MONKEY_BOMB_SPLASH.damage(1.25).unwrap(), 2725.0));
        assert!(close(MONKEY_BOMB_SPLASH.damage(2.5).unwrap(), 450.0));
        assert_eq!(MONKEY_BOMB_SPLASH.damage(2.6), None);
    }

    #[test]
    fn the_crawler_explosives_are_a_set_and_nothing_else_makes_crawlers() {
        for explosive in [Explosive::Frag, Explosive::Claymore, Explosive::MonkeyBomb, Explosive::GrenadeLauncher] {
            assert!(makes_crawlers(explosive), "{explosive:?}");
        }
        // Launchers, Ray Gun splash and the Crossbow (the Awful Lawton keeps its id).
        let weapons: Vec<_> = crate::tuning::WEAPONS
            .iter()
            .map(|row| row.id)
            .filter(|&id| makes_crawlers(Explosive::Weapon(id)))
            .collect();
        assert_eq!(weapons, [WeaponId::M72Law, WeaponId::ChinaLake, WeaponId::RayGun, WeaponId::Crossbow]);
        // Mustang & Sally fire grenades, but aren't on the list.
        assert!(!makes_crawlers(Explosive::Weapon(WeaponId::M1911)));
    }

    #[test]
    fn launchers_fire_projectiles_and_guns_fire_bullets() {
        let law = flight(WeaponId::M72Law).unwrap();
        assert_eq!(law.gravity, 0.0, "the rocket flies straight");
        assert!(flight(WeaponId::ChinaLake).unwrap().gravity > 0.0, "the grenade arcs");
        assert_eq!(flight(WeaponId::M1911), None);
        assert_eq!(flight(WeaponId::RayGun), None);
    }
}
