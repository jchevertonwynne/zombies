use avian3d::prelude::*;
use bevy::prelude::*;

use crate::layers::GameLayer;
use crate::player::{Locomotion, Player, PlayerCamera};
use crate::state::Pause;
use crate::tuning::*;
use crate::zombie::{Attack, DamageZombie, HitArea, HitZone};

#[derive(Component)]
pub struct Weapon {
    pub magazine: u32,
    pub magazine_size: u32,
    /// `None` means unlimited Reserve Ammo.
    pub reserve: Option<u32>,
    /// Seconds left on the current Reload, if reloading.
    pub reloading: Option<f32>,
    fire_cooldown: f32,
}

impl Weapon {
    pub fn pistol() -> Self {
        Self {
            magazine: PISTOL_MAGAZINE_SIZE,
            magazine_size: PISTOL_MAGAZINE_SIZE,
            reserve: None,
            reloading: None,
            fire_cooldown: 0.0,
        }
    }

    fn can_reload(&self) -> bool {
        self.reloading.is_none()
            && self.magazine < self.magazine_size
            && self.reserve.is_none_or(|r| r > 0)
    }

    fn finish_reload(&mut self) {
        let wanted = self.magazine_size - self.magazine;
        let taken = match &mut self.reserve {
            None => wanted,
            Some(reserve) => {
                let taken = wanted.min(*reserve);
                *reserve -= taken;
                taken
            }
        };
        self.magazine += taken;
        self.reloading = None;
    }
}

#[derive(Component, Default)]
pub struct Knife {
    cooldown: f32,
}

/// The pistol went off (for audio and effects).
#[derive(Message)]
pub struct ShotFired;

/// The on-screen pistol model, a child of the camera.
#[derive(Component)]
struct ViewModel {
    kick: f32,
}

pub struct WeaponPlugin;

impl Plugin for WeaponPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ShotFired>()
            .add_systems(Update, spawn_view_model)
            .add_systems(
                Update,
                (reload, fire, knife, animate_view_model)
                    .chain()
                    .run_if(in_state(Pause::Running)),
            );
    }
}

fn spawn_view_model(
    mut commands: Commands,
    cameras: Query<Entity, Added<PlayerCamera>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for camera in &cameras {
        let metal = materials.add(StandardMaterial {
            base_color: Color::srgb(0.15, 0.15, 0.17),
            metallic: 0.8,
            perceptual_roughness: 0.4,
            ..default()
        });
        commands.entity(camera).with_child((
            ViewModel { kick: 0.0 },
            Mesh3d(meshes.add(Cuboid::new(0.06, 0.1, 0.3))),
            MeshMaterial3d(metal),
            Transform::from_xyz(0.22, -0.2, -0.45),
        ));
    }
}

/// Resolves a collider hit to the Zombie it belongs to, and the hit zone.
fn zombie_for(hit: Entity, hit_areas: &Query<(&HitArea, &ChildOf)>) -> Option<(Entity, HitZone)> {
    hit_areas.get(hit).ok().map(|(area, parent)| (parent.parent(), area.0))
}

/// Attacks hit the world and Zombies' hit areas, never the body capsules they move with.
fn attack_filter(player: Entity) -> SpatialQueryFilter {
    SpatialQueryFilter::from_mask([GameLayer::World, GameLayer::ZombieHitArea]).with_excluded_entities([player])
}

fn reload(
    mut weapon: Single<&mut Weapon, With<Player>>,
    input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
) {
    if input.just_pressed(KeyCode::KeyR) && weapon.can_reload() {
        weapon.reloading = Some(PISTOL_RELOAD_SECS);
    }
    if let Some(left) = weapon.reloading {
        let left = left - time.delta_secs();
        if left <= 0.0 {
            weapon.finish_reload();
        } else {
            weapon.reloading = Some(left);
        }
    }
}

fn fire(
    player: Single<(Entity, &mut Weapon, &Locomotion), With<Player>>,
    camera: Single<&GlobalTransform, With<PlayerCamera>>,
    mut view_model: Single<&mut ViewModel>,
    mouse: Res<ButtonInput<MouseButton>>,
    spatial: SpatialQuery,
    hit_areas: Query<(&HitArea, &ChildOf)>,
    mut damage: MessageWriter<DamageZombie>,
    mut shots: MessageWriter<ShotFired>,
    time: Res<Time>,
) {
    let (entity, mut weapon, locomotion) = player.into_inner();
    weapon.fire_cooldown = (weapon.fire_cooldown - time.delta_secs()).max(0.0);

    let ready = weapon.fire_cooldown <= 0.0 && weapon.reloading.is_none() && !locomotion.sprinting;
    if !mouse.just_pressed(MouseButton::Left) || !ready {
        return;
    }
    if weapon.magazine == 0 {
        if weapon.can_reload() {
            weapon.reloading = Some(PISTOL_RELOAD_SECS);
        }
        return;
    }

    weapon.magazine -= 1;
    weapon.fire_cooldown = PISTOL_FIRE_COOLDOWN_SECS;
    view_model.kick = 1.0;
    shots.write(ShotFired);

    if let Some(hit) = spatial.cast_ray(
        camera.translation(),
        camera.forward(),
        PISTOL_RANGE,
        true,
        &attack_filter(entity),
    ) && let Some((zombie, zone)) = zombie_for(hit.entity, &hit_areas)
    {
        // Per-class multipliers for the other zones arrive with the combat model.
        let multiplier = if zone == HitZone::Head { HEADSHOT_MULTIPLIER } else { 1.0 };
        damage.write(DamageZombie {
            zombie,
            damage: PISTOL_DAMAGE * multiplier,
            zone,
            attack: Attack::Bullet,
        });
    }

    // Like the real thing: firing the last bullet starts a Reload.
    if weapon.magazine == 0 && weapon.can_reload() {
        weapon.reloading = Some(PISTOL_RELOAD_SECS);
    }
}

fn knife(
    player: Single<(Entity, &mut Knife), With<Player>>,
    camera: Single<&GlobalTransform, With<PlayerCamera>>,
    input: Res<ButtonInput<KeyCode>>,
    spatial: SpatialQuery,
    hit_areas: Query<(&HitArea, &ChildOf)>,
    mut damage: MessageWriter<DamageZombie>,
    time: Res<Time>,
) {
    let (entity, mut knife) = player.into_inner();
    knife.cooldown = (knife.cooldown - time.delta_secs()).max(0.0);
    if !input.just_pressed(KeyCode::KeyV) || knife.cooldown > 0.0 {
        return;
    }
    knife.cooldown = KNIFE_COOLDOWN_SECS;

    if let Some(hit) = spatial.cast_shape(
        &Collider::sphere(KNIFE_RADIUS),
        camera.translation(),
        Quat::IDENTITY,
        camera.forward(),
        &ShapeCastConfig::from_max_distance(KNIFE_RANGE),
        &attack_filter(entity),
    ) && let Some((zombie, zone)) = zombie_for(hit.entity, &hit_areas)
    {
        damage.write(DamageZombie {
            zombie,
            damage: KNIFE_DAMAGE,
            zone,
            attack: Attack::Knife,
        });
    }
}

fn animate_view_model(
    mut view_model: Single<(&mut ViewModel, &mut Transform)>,
    weapon: Single<&Weapon, With<Player>>,
    time: Res<Time>,
) {
    let (ref mut model, ref mut transform) = *view_model;
    model.kick = (model.kick - time.delta_secs() * 8.0).max(0.0);

    // Dip the gun out of view while reloading.
    let dip = match weapon.reloading {
        Some(left) => {
            let t = 1.0 - left / PISTOL_RELOAD_SECS;
            (t * std::f32::consts::PI).sin() * 0.25
        }
        None => 0.0,
    };
    transform.translation = Vec3::new(0.22, -0.2 - dip, -0.45 + model.kick * 0.05);
    transform.rotation = Quat::from_rotation_x(model.kick * 0.25 - dip * 2.0);
}
