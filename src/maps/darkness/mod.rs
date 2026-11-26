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
use crate::maps::power::Power;
use crate::maps::quarantine::{Quarantine, wing_of_zone};
use lamp_flicker::{LampBurst, LampFlicker};
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
            emissive: LinearRgba::rgb(CHALK_FAINT_GLOW, CHALK_FAINT_GLOW, CHALK_FAINT_GLOW),
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
    let pack_a_punch = layout.section_of(zone).is_none() && zone_at(map, machine_at(map)) == Some(zone);
    if pack_a_punch { generators.all_held() } else { generators.powers(Some(zone)) }
}

pub struct DarknessPlugin;

impl Plugin for DarknessPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChalkMaterials>()
            .add_message::<LampBurst>()
            .add_message::<flood::FloodOn>()
            .init_resource::<flood::FloodBeat>()
            .add_observer(light_camera)
            .add_systems(Update, (light_lamps, light_chalk, flood::light_floods).run_if(in_state(GameState::Playing)))
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

/// On a dark Map, the Player camera has HDR and a strong bloom, so what glows halos in the dark,
/// close black fog, and the local Player's Weapon Light. Both are there from the camera's first
/// frame, while the Map loads: the shaders bloom needs compile then, and never stall a Round by
/// being switched on later. (The ambient light is the Look's, which is none with Darkness.)
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
    power: Option<Res<Power>>,
    quarantine: Option<Res<Quarantine>>,
    time: Res<Time>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
    mut bursts: MessageWriter<LampBurst>,
    mut lamps: Query<(&mut Lamp, Option<&mut PointLight>, Option<&mut SpotLight>, Option<&mut LampFlicker>)>,
) {
    let powered = power.is_some_and(|power| power.is_on());
    for (mut lamp, point, spot, tube) in &mut lamps {
        let lit = lit(*map, &generators, lamp.zone);
        let target = if lit { 1.0 } else { 0.0 };
        let level = lamp.level.map_or(target, |level| fade(level, lit, time.delta_secs()));
        lamp.level = Some(level);
        // A Tube lamp's own flicker, and its glowing tube following it.
        let tube_factor = match tube {
            Some(mut tube) => {
                let (idle, began) = tube.factor(time.elapsed_secs(), powered);
                if began {
                    bursts.write(tube.burst_event());
                }
                // The Wing that is out has its own stages (a Quarantine, shown on every machine).
                let factor = match quarantine.as_deref() {
                    Some(quarantine) if quarantine.wing().is_some() && wing_of_zone(*map, lamp.zone) == quarantine.wing() => {
                        tube.wing_factor(quarantine.stage(), quarantine.elapsed(), time.elapsed_secs(), idle)
                    }
                    _ => idle,
                };
                let shown = level * factor;
                if tube.applied != shown {
                    tube.applied = shown;
                    if let (Some((handle, full)), Some(materials)) = (&tube.glow, materials.as_mut())
                        && let Some(mut material) = materials.get_mut(handle)
                    {
                        material.emissive = *full * shown;
                    }
                }
                factor
            }
            None => 1.0,
        };
        let strength = map.layout().look.lamps * level * tube_factor * if lit { flicker(level) } else { 1.0 };
        if let Some(mut light) = point {
            let intensity = LAMP_INTENSITY * strength;
            if light.intensity != intensity {
                light.intensity = intensity;
            }
        }
        if let Some(mut light) = spot {
            let intensity = SPOT_LAMP_INTENSITY * strength;
            if light.intensity != intensity {
                light.intensity = intensity;
            }
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
    if level < 1.0 && (level * LAMP_FLICKER_RATE).sin() < LAMP_FLICKER_BELOW { LAMP_FLICKER_DIP } else { 1.0 }
}

pub mod flood;
pub mod lamp_flicker;
pub mod weapon_light;

#[cfg(test)]
mod archive_tests;
#[cfg(test)]
mod refinery_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tube_tests;
