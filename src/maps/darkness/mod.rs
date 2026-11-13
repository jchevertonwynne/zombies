//! Darkness: on a Map whose `Layout` opts in, there's no ambient light and the fog is close and
//! black, so a Zone's lamps are its only light. A Zone in a Section is lit while its Section is
//! Powered, the Zone Pack-a-Punch is in waits for every Generator to be Held, and any other Zone
//! is always lit. Everything here follows `Generators`, so every Player sees the same Zones lit.

use avian3d::collider_tree::ColliderTrees;
use bevy::camera::Hdr;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;

use crate::enemies::nav::zone_at;
use crate::maps::generator::Generators;
use crate::maps::map::MapId;
use crate::maps::zone::ZoneId;
use crate::players::player::PlayerCamera;
use crate::state::GameState;
use crate::tuning::*;
use crate::weapons::pack_a_punch::machine_at;

/// A lamp the Map built, in `zone`. Its light's intensity is `LAMP_INTENSITY` scaled by how lit
/// its Zone is; lamps are never spawned or despawned as Zones go Dark.
#[derive(Component, Debug, Clone, Copy)]
pub struct Lamp {
    pub zone: ZoneId,
    /// How lit, from 0 off to 1 full, or `None` before its first frame.
    level: Option<f32>,
}

impl Lamp {
    pub fn in_zone(zone: ZoneId) -> Self {
        Self { zone, level: None }
    }
}

/// A chalk outline (a Wall Weapon's or an Equipment wall's) in its Zone. In a Dark Zone it glows
/// faintly, so it can still be found.
#[derive(Component, Debug, Clone, Copy)]
pub struct Chalk(pub ZoneId);

/// The chalk outlines' look: as they are, and glowing faintly for a Dark Zone.
#[derive(Resource)]
pub struct ChalkMaterials {
    pub plain: Handle<StandardMaterial>,
    glowing: Handle<StandardMaterial>,
}

impl FromWorld for ChalkMaterials {
    fn from_world(world: &mut World) -> Self {
        let plain = StandardMaterial {
            base_color: Color::srgb(0.9, 0.9, 0.85),
            perceptual_roughness: 1.0,
            ..default()
        };
        let glowing = StandardMaterial {
            emissive: LinearRgba::rgb(CHALK_GLOW, CHALK_GLOW, CHALK_GLOW),
            ..plain.clone()
        };
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        Self { plain: materials.add(plain), glowing: materials.add(glowing) }
    }
}

/// Whether `zone` on `map` is lit: always on a Map without Darkness; otherwise, a Zone in a Section
/// while that Section is Powered, Pack-a-Punch's Zone while every Generator is Held, and any other
/// Zone always.
pub fn lit(map: MapId, generators: &Generators, zone: ZoneId) -> bool {
    let layout = map.layout();
    if !layout.darkness {
        return true;
    }
    match layout.section_of(zone) {
        Some(section) => generators.section_powered(section),
        None if zone_at(machine_at(map)) == Some(zone) => generators.all_held(),
        None => true,
    }
}

pub struct DarknessPlugin;

impl Plugin for DarknessPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChalkMaterials>()
            .add_observer(light_camera)
            .add_systems(OnEnter(GameState::Playing), set_ambient)
            .add_systems(Update, (light_lamps, light_chalk).run_if(in_state(GameState::Playing)))
            .add_systems(
                PostUpdate,
                (
                    weapon_light::light_teammates,
                    weapon_light::aim_teammate_lights,
                    weapon_light::keep_out_of_walls.run_if(resource_exists::<ColliderTrees>),
                    weapon_light::place_weapon_lights,
                )
                    .chain()
                    .before(TransformSystems::Propagate)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(PostUpdate, weapon_light::view_model_casts_no_shadow);
    }
}

/// No ambient light on a dark Map, and the usual on every other.
fn set_ambient(map: Res<MapId>, mut ambient: ResMut<GlobalAmbientLight>) {
    ambient.brightness = if map.layout().darkness { 0.0 } else { AMBIENT_BRIGHTNESS };
}

/// On a dark Map, the Player camera has HDR and bloom, so what glows halos in the dark, close
/// black fog, and the local Player's Weapon Light. Both are there from the camera's first frame, while the Map loads: the shaders bloom
/// needs compile then, and never stall a Round by being switched on later.
fn light_camera(add: On<Add<PlayerCamera>>, map: Res<MapId>, mut commands: Commands) {
    if !map.layout().darkness {
        return;
    }
    commands.entity(add.entity).insert((
        Hdr,
        Bloom::NATURAL,
        DistanceFog {
            color: Color::BLACK,
            falloff: FogFalloff::Linear { start: DARK_FOG_START, end: DARK_FOG_END },
            ..default()
        },
    ));
    weapon_light::light_camera(add.entity, &mut commands);
}

/// Each chalk outline glows faintly while its Zone is Dark.
fn light_chalk(
    map: Res<MapId>,
    generators: Res<Generators>,
    looks: Res<ChalkMaterials>,
    mut chalks: Query<(&Chalk, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    for (&Chalk(zone), mut material) in &mut chalks {
        let look = if lit(*map, &generators, zone) { &looks.plain } else { &looks.glowing };
        if material.0 != *look {
            material.0 = look.clone();
        }
    }
}

/// Each lamp follows whether its Zone is lit: on its first frame at once, and after that fading
/// up with a flicker, as a Generator spins up, or dropping faster as one is lost.
fn light_lamps(
    map: Res<MapId>,
    generators: Res<Generators>,
    time: Res<Time>,
    mut lamps: Query<(&mut Lamp, &mut PointLight)>,
) {
    for (mut lamp, mut light) in &mut lamps {
        let lit = lit(*map, &generators, lamp.zone);
        let target = if lit { 1.0 } else { 0.0 };
        let level = lamp.level.map_or(target, |level| fade(level, lit, time.delta_secs()));
        lamp.level = Some(level);
        let intensity = LAMP_INTENSITY * level * if lit { flicker(level) } else { 1.0 };
        if light.intensity != intensity {
            light.intensity = intensity;
        }
    }
}

/// How lit a lamp is `dt` after `level`, heading lit or Dark.
fn fade(level: f32, lit: bool, dt: f32) -> f32 {
    if lit {
        (level + dt / LAMP_FADE_UP_SECS).min(1.0)
    } else {
        (level - dt / LAMP_FADE_DOWN_SECS).max(0.0)
    }
}

/// How much of its light a lamp coming on at `level` gives: it stutters on the way up, the same
/// way for every lamp, and is steady once full.
fn flicker(level: f32) -> f32 {
    if level < 1.0 && (level * LAMP_FLICKER_RATE).sin() < -0.3 { LAMP_FLICKER_DIP } else { 1.0 }
}

pub mod weapon_light;

#[cfg(test)]
mod tests;
