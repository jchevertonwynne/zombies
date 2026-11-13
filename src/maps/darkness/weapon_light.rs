//! Weapon Lights: on a Map with Darkness every Player carries a narrow, always-on spot light that
//! casts shadows. The local Player's hangs off the camera; each teammate's hangs off their body at
//! eye height and points where they look, so it shows from outside the cone and from another Zone.
//!
//! Each light sits ahead of its Player's muzzle, so it never lights their own Weapon or throws its
//! shadow, but never inside a wall, or it would light the Zone on the far side. On a dark Map the
//! view model casts no shadow at all.
//!
//! Every Weapon Light is there from the moment its Player is, shadows and all, and is never
//! switched: the shadow-casting shaders compile while the Map loads, not mid-Round.

use avian3d::prelude::*;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use crate::layers::GameLayer;
use crate::maps::map::MapId;
use crate::players::player::{LocalPlayer, Locomotion, MoveInput, Player, PlayerCamera};
use crate::tuning::*;

/// A Player's Weapon Light. It sits `reach` along `look` from `eye`, both in its parent's space
/// (the camera's, or a teammate's body's).
#[derive(Component, Debug, Clone, Copy)]
pub struct WeaponLight {
    eye: Vec3,
    look: Vec3,
    reach: f32,
}

impl WeaponLight {
    fn at(eye: Vec3, look: Vec3) -> Self {
        Self { eye, look, reach: WEAPON_LIGHT_AHEAD }
    }

    fn transform(&self) -> Transform {
        Transform::from_translation(self.eye + self.look * self.reach).looking_to(self.look, Vec3::Y)
    }
}

fn spot() -> SpotLight {
    SpotLight {
        color: WEAPON_LIGHT_COLOR,
        intensity: WEAPON_LIGHT_INTENSITY,
        range: WEAPON_LIGHT_RANGE,
        radius: 0.05,
        inner_angle: WEAPON_LIGHT_INNER_ANGLE,
        outer_angle: WEAPON_LIGHT_OUTER_ANGLE,
        shadow_maps_enabled: true,
        ..default()
    }
}

/// A Weapon Light hanging from `parent`, from `eye` along `look`.
fn weapon_light(parent: Entity, eye: Vec3, look: Vec3) -> impl Bundle {
    let light = WeaponLight::at(eye, look);
    (light, spot(), light.transform(), ChildOf(parent))
}

/// On a dark Map, the local Player's Weapon Light, on the camera from its first frame.
pub(super) fn light_camera(camera: Entity, commands: &mut Commands) {
    commands.spawn(weapon_light(camera, Vec3::ZERO, Vec3::NEG_Z));
}

/// On a dark Map, each teammate gets their Weapon Light as they arrive.
pub(super) fn light_teammates(
    map: Res<MapId>,
    teammates: Query<Entity, (Added<Player>, Without<LocalPlayer>)>,
    mut commands: Commands,
) {
    if !map.layout().darkness {
        return;
    }
    for body in &teammates {
        commands.spawn(weapon_light(body, Vec3::ZERO, Vec3::NEG_Z));
    }
}

/// A teammate's light follows their eye as their Stance changes, and points where they look.
pub(super) fn aim_teammate_lights(
    mut lights: Query<(&ChildOf, &mut WeaponLight)>,
    players: Query<(&Locomotion, &MoveInput), Without<LocalPlayer>>,
) {
    for (parent, mut light) in &mut lights {
        let Ok((locomotion, input)) = players.get(parent.parent()) else {
            continue;
        };
        light.eye = Vec3::Y * (locomotion.eye - locomotion.shape().half_height());
        light.look = Quat::from_rotation_x(input.pitch) * Vec3::NEG_Z;
    }
}

/// Each light comes in short of any wall within its reach, so it never lights the far side.
pub(super) fn keep_out_of_walls(
    spatial: SpatialQuery,
    mut lights: Query<(&ChildOf, &mut WeaponLight)>,
    parents: Query<&GlobalTransform>,
) {
    let walls = SpatialQueryFilter::from_mask(GameLayer::World);
    for (parent, mut light) in &mut lights {
        let Ok(parent) = parents.get(parent.parent()) else {
            continue;
        };
        let Ok(look) = Dir3::new(parent.affine().transform_vector3(light.look)) else {
            continue;
        };
        let from = parent.transform_point(light.eye);
        let wall = spatial.cast_ray(from, look, WEAPON_LIGHT_AHEAD, true, &walls).map(|hit| hit.distance);
        light.reach = reach(wall);
    }
}

/// How far ahead of the eye a Weapon Light sits, with a wall `wall` metres ahead (if one is in
/// reach): ahead of the muzzle, or just short of the wall.
pub(super) fn reach(wall: Option<f32>) -> f32 {
    match wall {
        Some(wall) => (wall - WEAPON_LIGHT_WALL_GAP).clamp(0.0, WEAPON_LIGHT_AHEAD),
        None => WEAPON_LIGHT_AHEAD,
    }
}

/// Puts each light where its eye, look and reach say.
pub(super) fn place_weapon_lights(mut lights: Query<(&WeaponLight, &mut Transform), Changed<WeaponLight>>) {
    for (light, mut transform) in &mut lights {
        *transform = light.transform();
    }
}

/// On a dark Map, nothing on the camera (the view model, its hands and parts) casts a shadow, so
/// no light, the Player's own above all, throws the Weapon's shadow across the world.
pub(super) fn view_model_casts_no_shadow(
    map: Option<Res<MapId>>,
    cameras: Query<Entity, With<PlayerCamera>>,
    children: Query<&Children>,
    meshes: Query<(), (With<Mesh3d>, Without<NotShadowCaster>)>,
    mut commands: Commands,
) {
    if !map.is_some_and(|map| map.layout().darkness) {
        return;
    }
    for camera in &cameras {
        for part in children.iter_descendants(camera) {
            if meshes.contains(part) {
                commands.entity(part).insert(NotShadowCaster);
            }
        }
    }
}
