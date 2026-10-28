//! What a shot looks like: a short muzzle flash and light on the held Weapon, puffs where hitscan
//! bullets land (grey on the world, red on a Zombie, a bigger burst on a Headshot), and a faint
//! tracer on every few bullets. All of it is cosmetic and reads `ShotFired` and `BulletImpact`.

use bevy::prelude::*;

use crate::arsenal::WeaponClass;
use crate::gun_model::model_length;
use crate::state::Pause;
use crate::tuning::*;
use crate::weapon::{ShotFired, ViewModel};

/// What a bullet hit, which decides the colour and size of its puffs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImpactKind {
    World,
    Zombie,
    Headshot,
}

/// A hitscan bullet landed at `at` on a surface facing `normal`.
#[derive(Message, Debug, Clone, Copy)]
pub struct BulletImpact {
    pub at: Vec3,
    pub normal: Vec3,
    pub kind: ImpactKind,
}

/// How a class's muzzle flash looks: its size and colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlashLook {
    pub size: f32,
    pub colour: Color,
}

/// The muzzle flash for a class of Weapon: bigger for Shotguns and LMGs, tinted for the Ray Gun,
/// Thundergun and launchers, and none for the quiet Crossbow and Ballistic Knife.
pub fn flash_look(class: WeaponClass) -> Option<FlashLook> {
    let warm = Color::srgb(1.0, 0.8, 0.4);
    let (size, colour) = match class {
        WeaponClass::Pistol | WeaponClass::DualWield => (MUZZLE_FLASH_SIZE_SMALL, warm),
        WeaponClass::Smg | WeaponClass::Rifle | WeaponClass::Sniper => (MUZZLE_FLASH_SIZE_MEDIUM, warm),
        WeaponClass::Shotgun | WeaponClass::Lmg => (MUZZLE_FLASH_SIZE_LARGE, warm),
        WeaponClass::Launcher => (MUZZLE_FLASH_SIZE_LARGE, Color::srgb(1.0, 0.5, 0.2)),
        WeaponClass::RayGun => (MUZZLE_FLASH_SIZE_MEDIUM, Color::srgb(0.4, 1.0, 0.5)),
        WeaponClass::Thundergun => (MUZZLE_FLASH_SIZE_LARGE, Color::srgb(0.5, 0.7, 1.0)),
        WeaponClass::Crossbow | WeaponClass::BallisticKnife => return None,
    };
    Some(FlashLook { size, colour })
}

/// Where the muzzle sits on the view model for a class, as an offset from the receiver.
pub fn muzzle_position(class: WeaponClass) -> Vec3 {
    Vec3::new(0.0, 0.01, -0.15 * model_length(class))
}

/// How much of its size an effect still has `age` seconds into its `life`: 1 at the start,
/// shrinking quickly to 0 at the end.
pub fn fade(age: f32, life: f32) -> f32 {
    if life <= 0.0 {
        return 0.0;
    }
    let left = (1.0 - age / life).clamp(0.0, 1.0);
    left * left
}

/// How many puffs an impact throws.
pub fn puff_count(kind: ImpactKind) -> u32 {
    match kind {
        ImpactKind::World => IMPACT_PUFFS_WORLD,
        ImpactKind::Zombie => IMPACT_PUFFS_ZOMBIE,
        ImpactKind::Headshot => IMPACT_PUFFS_HEADSHOT,
    }
}

/// How many more puffs may spawn with `alive` already out there, wanting `wanted`.
pub fn puffs_allowed(alive: usize, wanted: u32) -> u32 {
    (IMPACT_PUFF_CAP.saturating_sub(alive) as u32).min(wanted)
}

/// A transform for a unit cube stretched into a line from `from` to `to`, `width` across.
pub fn tracer_transform(from: Vec3, to: Vec3, width: f32) -> Transform {
    let line = to - from;
    let length = line.length().max(0.001);
    let direction = line / length;
    let up = if direction.y.abs() > 0.99 { Vec3::X } else { Vec3::Y };
    Transform::from_translation(from.midpoint(to))
        .looking_to(direction, up)
        .with_scale(Vec3::new(width, width, length))
}

/// The flash at the muzzle of the view model, with its light. Hidden between shots.
#[derive(Component)]
struct MuzzleFlash {
    material: Handle<StandardMaterial>,
    size: f32,
    age: f32,
}

/// A flying puff of dust or blood.
#[derive(Component)]
struct Puff {
    age: f32,
    size: f32,
    velocity: Vec3,
}

/// A faint line from the muzzle to where a bullet landed.
#[derive(Component)]
struct Tracer {
    age: f32,
}

/// The meshes and materials shared by every puff and tracer.
#[derive(Resource)]
struct EffectAssets {
    cube: Handle<Mesh>,
    dust: Handle<StandardMaterial>,
    blood: Handle<StandardMaterial>,
    headshot: Handle<StandardMaterial>,
    tracer: Handle<StandardMaterial>,
}

pub struct MuzzlePlugin;

impl Plugin for MuzzlePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<BulletImpact>()
            .add_systems(Startup, setup_assets)
            .add_systems(
                Update,
                (attach_flash, start_flash, update_flash, spawn_impacts, update_puffs, update_tracers)
                    .chain()
                    .run_if(in_state(Pause::Running)),
            );
    }
}

fn unlit(colour: Color) -> StandardMaterial {
    StandardMaterial {
        base_color: colour,
        emissive: LinearRgba::from(colour),
        unlit: true,
        ..default()
    }
}

fn setup_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(EffectAssets {
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        dust: materials.add(unlit(Color::srgb(0.55, 0.53, 0.5))),
        blood: materials.add(unlit(Color::srgb(0.65, 0.04, 0.04))),
        headshot: materials.add(unlit(Color::srgb(0.9, 0.08, 0.06))),
        tracer: materials.add(unlit(Color::srgb(1.0, 0.9, 0.6))),
    });
}

/// Gives each new view model its flash, as a child.
fn attach_flash(
    mut commands: Commands,
    view_models: Query<Entity, Added<ViewModel>>,
    assets: Res<EffectAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for view_model in &view_models {
        let material = materials.add(unlit(Color::WHITE));
        commands.entity(view_model).with_child((
            MuzzleFlash { material: material.clone(), size: 0.0, age: MUZZLE_FLASH_SECS },
            Mesh3d(assets.cube.clone()),
            MeshMaterial3d(material),
            PointLight {
                intensity: 0.0,
                range: MUZZLE_LIGHT_RANGE,
                ..default()
            },
            Transform::default(),
            Visibility::Hidden,
        ));
    }
}

/// A shot lights the flash at the muzzle, turned a random way so no two look alike.
fn start_flash(
    mut shots: MessageReader<ShotFired>,
    mut flashes: Query<(&mut MuzzleFlash, &mut Transform, &mut Visibility, &mut PointLight)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(shot) = shots.read().last() else { return };
    // An Underbarrel fires as its own class of gun.
    let class = match shot.underbarrel {
        Some(underbarrel) => underbarrel.stats().class,
        None => shot.weapon.stats().class,
    };
    let Some(look) = flash_look(class) else { return };
    for (mut flash, mut transform, mut visibility, mut light) in &mut flashes {
        flash.age = 0.0;
        flash.size = look.size;
        *visibility = Visibility::Inherited;
        // Centred a little past the muzzle so the flash sits in front of the barrel.
        let at = muzzle_position(class) - Vec3::Z * look.size * 0.5;
        *transform = Transform::from_translation(at)
            .with_rotation(Quat::from_rotation_z(rand::random::<f32>() * std::f32::consts::TAU))
            .with_scale(Vec3::new(look.size, look.size, look.size * 0.6));
        light.color = look.colour;
        light.intensity = MUZZLE_LIGHT_INTENSITY;
        if let Some(mut material) = materials.get_mut(&flash.material) {
            material.base_color = look.colour;
            material.emissive = LinearRgba::from(look.colour) * 4.0;
        }
    }
}

/// The flash and its light shrink away, then hide.
fn update_flash(
    mut flashes: Query<(&mut MuzzleFlash, &mut Transform, &mut Visibility, &mut PointLight)>,
    time: Res<Time>,
) {
    for (mut flash, mut transform, mut visibility, mut light) in &mut flashes {
        if flash.age >= MUZZLE_FLASH_SECS {
            continue;
        }
        flash.age += time.delta_secs();
        let left = fade(flash.age, MUZZLE_FLASH_SECS);
        if left <= 0.0 {
            *visibility = Visibility::Hidden;
            light.intensity = 0.0;
            continue;
        }
        let size = flash.size * left;
        transform.scale = Vec3::new(size, size, size * 0.6);
        light.intensity = MUZZLE_LIGHT_INTENSITY * left;
    }
}

/// Each bullet that lands throws puffs, within the cap, and every few draw a tracer.
fn spawn_impacts(
    mut commands: Commands,
    mut impacts: MessageReader<BulletImpact>,
    assets: Res<EffectAssets>,
    alive: Query<(), With<Puff>>,
    flashes: Query<&GlobalTransform, With<MuzzleFlash>>,
    mut landed: Local<u32>,
) {
    let mut count = alive.iter().count();
    for impact in impacts.read() {
        let (material, size) = match impact.kind {
            ImpactKind::World => (&assets.dust, IMPACT_PUFF_SIZE),
            ImpactKind::Zombie => (&assets.blood, IMPACT_PUFF_SIZE),
            ImpactKind::Headshot => (&assets.headshot, IMPACT_PUFF_SIZE_HEADSHOT),
        };
        let puffs = puffs_allowed(count, puff_count(impact.kind));
        count += puffs as usize;
        for _ in 0..puffs {
            let scatter = Vec3::new(
                rand::random::<f32>() - 0.5,
                rand::random::<f32>() - 0.3,
                rand::random::<f32>() - 0.5,
            );
            commands.spawn((
                Puff {
                    age: 0.0,
                    size,
                    velocity: (impact.normal + scatter) * IMPACT_PUFF_SPEED * (0.4 + rand::random::<f32>() * 0.6),
                },
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(material.clone()),
                Transform::from_translation(impact.at + impact.normal * 0.02).with_scale(Vec3::splat(size)),
            ));
        }
        *landed += 1;
        if landed.is_multiple_of(TRACER_EVERY)
            && let Ok(muzzle) = flashes.single()
        {
            commands.spawn((
                Tracer { age: 0.0 },
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(assets.tracer.clone()),
                tracer_transform(muzzle.translation(), impact.at, TRACER_WIDTH),
            ));
        }
    }
}

/// Puffs drift out, fall a little, shrink and vanish.
fn update_puffs(mut commands: Commands, mut puffs: Query<(Entity, &mut Puff, &mut Transform)>, time: Res<Time>) {
    let dt = time.delta_secs();
    for (entity, mut puff, mut transform) in &mut puffs {
        puff.age += dt;
        if puff.age >= IMPACT_PUFF_SECS {
            commands.entity(entity).despawn();
            continue;
        }
        puff.velocity.y -= GRAVITY * 0.3 * dt;
        transform.translation += puff.velocity * dt;
        transform.scale = Vec3::splat(puff.size * fade(puff.age, IMPACT_PUFF_SECS));
    }
}

/// Tracers thin out over their short life.
fn update_tracers(mut commands: Commands, mut tracers: Query<(Entity, &mut Tracer, &mut Transform)>, time: Res<Time>) {
    for (entity, mut tracer, mut transform) in &mut tracers {
        tracer.age += time.delta_secs();
        if tracer.age >= TRACER_SECS {
            commands.entity(entity).despawn();
            continue;
        }
        let thin = fade(tracer.age, TRACER_SECS);
        transform.scale.x = TRACER_WIDTH * thin;
        transform.scale.y = TRACER_WIDTH * thin;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shotguns_and_lmgs_flash_bigger_than_pistols() {
        let size = |class| flash_look(class).unwrap().size;
        assert!(size(WeaponClass::Shotgun) > size(WeaponClass::Rifle));
        assert!(size(WeaponClass::Lmg) > size(WeaponClass::Pistol));
        assert!(size(WeaponClass::Rifle) > size(WeaponClass::Pistol));
    }

    #[test]
    fn quiet_weapons_have_no_flash() {
        assert!(flash_look(WeaponClass::Crossbow).is_none());
        assert!(flash_look(WeaponClass::BallisticKnife).is_none());
    }

    #[test]
    fn muzzle_sits_further_out_on_longer_models() {
        assert!(muzzle_position(WeaponClass::Lmg).z < muzzle_position(WeaponClass::Pistol).z);
        assert!((muzzle_position(WeaponClass::Pistol).z + 0.15).abs() < 1e-6);
    }

    #[test]
    fn fade_runs_from_one_to_zero() {
        assert_eq!(fade(0.0, 0.2), 1.0);
        assert!(fade(0.1, 0.2) < 0.5);
        assert_eq!(fade(0.2, 0.2), 0.0);
        assert_eq!(fade(5.0, 0.2), 0.0);
        assert_eq!(fade(0.0, 0.0), 0.0);
    }

    #[test]
    fn headshots_burst_bigger_and_the_cap_holds() {
        assert!(puff_count(ImpactKind::Headshot) > puff_count(ImpactKind::Zombie));
        assert!(puff_count(ImpactKind::Zombie) >= puff_count(ImpactKind::World));
        assert_eq!(puffs_allowed(0, 5), 5);
        assert_eq!(puffs_allowed(IMPACT_PUFF_CAP - 2, 5), 2);
        assert_eq!(puffs_allowed(IMPACT_PUFF_CAP + 10, 5), 0);
    }

    #[test]
    fn tracer_spans_the_line() {
        let transform = tracer_transform(Vec3::ZERO, Vec3::new(0.0, 0.0, -10.0), 0.01);
        assert!((transform.scale.z - 10.0).abs() < 1e-5);
        assert!((transform.translation.z + 5.0).abs() < 1e-5);
        // Straight up doesn't degenerate.
        let up = tracer_transform(Vec3::ZERO, Vec3::Y * 4.0, 0.01);
        assert!(up.rotation.is_finite());
    }
}
