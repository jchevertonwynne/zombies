//! How each Map is lit and graded: its `Look` (in its `Layout`), over one post stack shared by
//! every Map. The sun, the ambient, the sky and fog colour and how strong the lamps are come from
//! the Look; the Player camera gets HDR, SMAA (MSAA off), SSAO, a faint Bloom, TonyMcMapface and
//! the Look's grade. A Map with Darkness keeps its own close black fog and stronger Bloom.
//!
//! A Look may later name a CC0 HDRI sky; until one is approved, its sky colour is the clear colour.

use bevy::anti_alias::smaa::Smaa;
use bevy::camera::Hdr;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{CascadeShadowConfig, CascadeShadowConfigBuilder};
use bevy::pbr::{DistanceFog, ScreenSpaceAmbientOcclusion};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::view::ColorGrading;

use crate::maps::map::MapId;
use crate::players::player::PlayerCamera;
use crate::state::GameState;
use crate::tuning::AMBIENT_BRIGHTNESS;

/// A Map's lighting and grade.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    /// The sun's (or moon's) colour and strength, in lux.
    pub sun_colour: Color,
    pub sun_lux: f32,
    /// How high it stands, as a factor on the steepness of the Layout's `sun`: below 1 is lower.
    pub sun_height: f32,
    /// Its shadow cascades and how far they reach, or Bevy's defaults.
    pub cascades: Option<(usize, f32)>,
    pub ambient_colour: Color,
    pub ambient_brightness: f32,
    /// The sky's colour, which the fog fades to.
    pub sky: Color,
    /// How strong the lamps are, as a fraction of `LAMP_INTENSITY`.
    pub lamps: f32,
    /// The grade's white balance: warmer above 0, and greener above 0.
    pub temperature: f32,
    pub tint: f32,
}

/// The look every Map had before Looks: a dim, cold moon over a near-black night, the lamps full.
pub const NIGHT: Look = Look {
    sun_colour: Color::srgb(0.7, 0.75, 1.0),
    sun_lux: 800.0,
    sun_height: 1.0,
    cascades: None,
    ambient_colour: Color::srgb(0.6, 0.65, 0.8),
    ambient_brightness: AMBIENT_BRIGHTNESS,
    sky: Color::srgb(0.02, 0.02, 0.04),
    lamps: 1.0,
    temperature: 0.0,
    tint: 0.0,
};

/// Dusk (#277): a low, warm, shadowed sun, a cool sky ambient, dusky mauve-grey fog, the lamps
/// barely needed, and a slightly warm grade.
pub const DUSK: Look = Look {
    sun_colour: Color::srgb(1.0, 0.62, 0.38),
    sun_lux: 4000.0,
    sun_height: 0.2,
    cascades: Some((2, 45.0)),
    ambient_colour: Color::srgb(0.55, 0.6, 0.75),
    ambient_brightness: 450.0,
    sky: Color::srgb(0.28, 0.25, 0.28),
    lamps: 0.15,
    temperature: 0.05,
    tint: 0.02,
};

/// The shared Bloom: just enough for bulbs and signs to glow.
const BLOOM: f32 = 0.05;

impl Look {
    /// The way the sun shines, given the Layout's `sun`.
    pub fn sun_direction(&self, sun: Vec3) -> Vec3 {
        Vec3::new(sun.x, sun.y * self.sun_height, sun.z)
    }

    /// The sun's shadow cascades.
    pub fn cascade_config(&self) -> CascadeShadowConfig {
        match self.cascades {
            Some((num_cascades, maximum_distance)) => CascadeShadowConfigBuilder {
                num_cascades,
                maximum_distance,
                first_cascade_far_bound: (maximum_distance / 4.0).min(12.0),
                ..default()
            }
            .into(),
            None => CascadeShadowConfigBuilder::default().into(),
        }
    }

    /// The ambient light's brightness: none on a Map with Darkness.
    pub fn ambient(&self, darkness: bool) -> f32 {
        if darkness { 0.0 } else { self.ambient_brightness }
    }
}

pub struct LookPlugin;

impl Plugin for LookPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(post_stack).add_systems(OnEnter(GameState::Playing), set_world);
    }
}

/// The sky and the ambient light, for the Map about to be played.
fn set_world(map: Res<MapId>, mut ambient: ResMut<GlobalAmbientLight>, clear: Option<ResMut<ClearColor>>) {
    let layout = map.layout();
    ambient.color = layout.look.ambient_colour;
    ambient.brightness = layout.look.ambient(layout.darkness);
    if let Some(mut clear) = clear {
        clear.0 = layout.look.sky;
    }
}

/// The post stack, from the camera's first frame so its shaders compile while the Map loads.
fn post_stack(add: On<Add<PlayerCamera>>, map: Res<MapId>, mut commands: Commands, mut cameras: Query<(&mut ColorGrading, &mut DistanceFog)>) {
    let layout = map.layout();
    let look = layout.look;
    let mut camera = commands.entity(add.entity);
    camera.insert((Hdr, Msaa::Off, Smaa::default(), ScreenSpaceAmbientOcclusion::default(), Tonemapping::TonyMcMapface));
    if layout.darkness {
        // Darkness brings its own Bloom and fog.
        return;
    }
    camera.insert(Bloom { intensity: BLOOM, ..Bloom::NATURAL });
    if let Ok((mut grading, mut fog)) = cameras.get_mut(add.entity) {
        grading.global.temperature = look.temperature;
        grading.global.tint = look.tint;
        fog.color = look.sky;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dusk_is_low_and_warm_and_night_is_as_it_was() {
        let sun = Vec3::new(-1.0, -3.0, -2.0);
        assert_eq!(NIGHT.sun_direction(sun), sun);
        let low = DUSK.sun_direction(sun);
        assert!(low.y > sun.y && low.y < 0.0, "still above the horizon, but lower: {low}");
        let warm = DUSK.sun_colour.to_srgba();
        assert!(warm.red > warm.blue);
    }

    #[test]
    fn a_dark_map_has_no_ambient_light_and_the_others_keep_their_looks() {
        assert_eq!(NIGHT.ambient(true), 0.0);
        assert_eq!(DUSK.ambient(false), DUSK.ambient_brightness);
        assert_eq!(MapId::Greybox.layout().look, DUSK);
        for map in [MapId::Quarry, MapId::Village, MapId::Dam] {
            assert_eq!(map.layout().look, NIGHT, "{map:?} keeps the night until its own rollout step");
        }
    }

    #[test]
    fn dusk_casts_two_cascades_out_to_the_fog() {
        let config = DUSK.cascade_config();
        assert_eq!(config.bounds.len(), 2);
        assert!((config.bounds[1] - 45.0).abs() < 1e-3);
    }
}
