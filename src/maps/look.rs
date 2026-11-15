//! How each Map is lit and graded: its `Look` (in its `Layout`), over one post stack shared by
//! every Map. The sun, the ambient, the sky and fog colour and how strong the lamps are come from
//! the Look; the Player camera gets HDR, SMAA (MSAA off), SSAO, a faint Bloom, TonyMcMapface and
//! the Look's grade. A Map with Darkness keeps its own close black fog and stronger Bloom.
//!
//! A Look may name a CC0 HDRI sky (a KTX2 cubemap from `texpack --sky`), which is the Player
//! camera's skybox and, filtered on the GPU, its outdoor reflections and ambient. If the Look names
//! none, or the file isn't on disk, the sky colour is the clear colour.

use bevy::anti_alias::smaa::Smaa;
use bevy::camera::Hdr;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{CascadeShadowConfig, CascadeShadowConfigBuilder, GeneratedEnvironmentMapLight, Skybox};
use bevy::pbr::{DistanceFog, ScreenSpaceAmbientOcclusion};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::view::ColorGrading;

use crate::maps::darkness::Lamp;
use crate::maps::map::{Layout, MapId};
use crate::maps::zone::{PlayerZone, ZoneId};
use crate::maps::map::textures::on_disk;
use crate::players::player::{LocalPlayer, PlayerCamera};
use crate::state::GameState;
use crate::tuning::{AMBIENT_BRIGHTNESS, FOG_COLOUR_RATE, SHADOWED_LAMPS};

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
    /// The sky's colour: what shows where there's no skybox.
    pub sky: Color,
    /// The distance fog's colour out of doors, which the horizon meets.
    pub fog: Color,
    /// An HDRI cubemap for the sky (a path under `assets/`) and how bright it shows, in cd/m2.
    pub sky_map: Option<(&'static str, f32)>,
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
    fog: Color::srgb(0.02, 0.02, 0.04),
    sky_map: None,
    lamps: 1.0,
    temperature: 0.0,
    tint: 0.0,
};

/// The Dam (#379): the night, under a dark, starry CC0 sky seen from the Crest Road. Its ambient
/// is the Darkness's none, and the sky lights nothing, so only the lamps and the moon do.
pub const DAM_NIGHT: Look = Look {
    // Poly Haven's Qwantani Night (Pure Sky), a deep blue-black with a faint glow low down.
    sky_map: Some(("skies/qwantani_night_puresky/sky.ktx2", 180.0)),
    ..NIGHT
};

/// Dusk (#277): a low, warm, shadowed sun, a cool sky ambient, dusky mauve-grey fog, the lamps
/// barely needed, and a slightly warm grade.
pub const DUSK: Look = Look {
    sun_colour: Color::srgb(1.0, 0.62, 0.38),
    sun_lux: 4000.0,
    sun_height: 0.2,
    cascades: Some((2, 45.0)),
    ambient_colour: Color::srgb(0.55, 0.6, 0.75),
    ambient_brightness: 250.0,
    sky: Color::srgb(0.28, 0.25, 0.28),
    fog: Color::srgb(0.24, 0.18, 0.2),
    // Poly Haven's Belfast Sunset (Pure Sky), turned to put its sun where the Layout's is.
    sky_map: Some(("skies/belfast_sunset_puresky/sky.ktx2", 260.0)),
    lamps: 1.0,
    temperature: 0.05,
    tint: 0.02,
};

/// How strongly the sky lights the Map, as a fraction of how bright it shows.
const SKY_LIGHT: f32 = 0.7;

/// The fog's colour while the eye is indoors on a roofed Map: near-black, so a room doesn't glow
/// the sky's colour.
const INDOOR_FOG: Color = Color::srgb(0.02, 0.018, 0.018);

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
        app.add_observer(post_stack)
            .add_systems(OnEnter(GameState::Playing), set_world)
            .add_systems(Update, light_from_sky)
            .add_systems(Update, (follow_zone_fog, shadow_nearest_lamps).run_if(in_state(GameState::Playing)));
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

/// The fog colour the eye wants in `zone`: near-black indoors, the Look's out of doors. Maps
/// without ceilings (and with Darkness's black fog) keep their Look's.
pub fn fog_colour(layout: &Layout, zone: ZoneId) -> Color {
    if layout.roofed && !layout.darkness && layout.indoor(zone) { INDOOR_FOG } else { layout.look.fog }
}

/// On a roofed Map the fog's colour closes on the eye's Zone's colour, settling in about half a
/// second, so stepping through a Door fades it.
fn follow_zone_fog(
    map: Res<MapId>,
    time: Res<Time>,
    zones: Query<&PlayerZone, With<LocalPlayer>>,
    mut fogs: Query<&mut DistanceFog, With<PlayerCamera>>,
) {
    let layout = map.layout();
    if !layout.roofed || layout.darkness {
        return;
    }
    let Some(zone) = zones.iter().next() else { return };
    let target = LinearRgba::from(fog_colour(layout, zone.0));
    let by = (time.delta_secs() * FOG_COLOUR_RATE).min(1.0);
    for mut fog in &mut fogs {
        let now = LinearRgba::from(fog.color);
        if now != target {
            fog.color = now.mix(&target, by).into();
        }
    }
}

/// Only the spot lamps nearest the eye cast shadows.
fn shadow_nearest_lamps(eyes: Query<&GlobalTransform, With<PlayerCamera>>, mut lamps: Query<(&GlobalTransform, &mut SpotLight), With<Lamp>>) {
    let Some(eye) = eyes.iter().next().map(GlobalTransform::translation) else { return };
    let mut by_distance: Vec<_> = lamps.iter_mut().map(|(at, light)| (at.translation().distance_squared(eye), light)).collect();
    by_distance.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (rank, (_, mut light)) in by_distance.into_iter().enumerate() {
        let shadowed = rank < SHADOWED_LAMPS;
        if light.shadow_maps_enabled != shadowed {
            light.shadow_maps_enabled = shadowed;
        }
    }
}

/// A camera whose sky is still to light the Map, at this intensity.
#[derive(Component)]
struct SkyLight(f32);

/// Once the sky has loaded, filter a copy of it into reflections and ambient light. (Sharing the
/// skybox's own image with the filter blanks the view.)
fn light_from_sky(mut commands: Commands, mut images: ResMut<Assets<Image>>, cameras: Query<(Entity, &Skybox, &SkyLight)>) {
    for (entity, skybox, light) in &cameras {
        let Some(sky) = skybox.image.as_ref().and_then(|handle| images.get(handle)).cloned() else { continue };
        let copy = images.add(sky);
        commands.entity(entity).remove::<SkyLight>().insert(GeneratedEnvironmentMapLight { environment_map: copy, intensity: light.0, ..default() });
    }
}

/// The post stack, from the camera's first frame so its shaders compile while the Map loads.
fn post_stack(
    add: On<Add<PlayerCamera>>,
    map: Res<MapId>,
    asset_server: Option<Res<AssetServer>>,
    mut commands: Commands,
    mut cameras: Query<(&mut ColorGrading, &mut DistanceFog)>,
) {
    let layout = map.layout();
    let look = layout.look;
    let mut camera = commands.entity(add.entity);
    camera.insert((Hdr, Msaa::Off, Smaa::default(), ScreenSpaceAmbientOcclusion::default(), Tonemapping::TonyMcMapface));
    if !layout.darkness {
        camera.insert(Bloom { intensity: BLOOM, ..Bloom::NATURAL });
    }
    if let (Some((file, brightness)), Some(asset_server)) = (look.sky_map, asset_server)
        && on_disk(file)
    {
        let image: Handle<Image> = asset_server.load(file);
        camera.insert(Skybox { image: Some(image), brightness, ..default() });
        // Under Darkness the sky is only to look at: it lights nothing.
        if !layout.darkness {
            camera.insert(SkyLight(brightness * SKY_LIGHT));
        }
    }
    if layout.darkness {
        // Darkness brings its own Bloom and fog.
        return;
    }
    if let Ok((mut grading, mut fog)) = cameras.get_mut(add.entity) {
        grading.global.temperature = look.temperature;
        grading.global.tint = look.tint;
        fog.color = fog_colour(layout, layout.start_zone());
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
        for map in [MapId::Quarry, MapId::Village, MapId::Cove] {
            assert_eq!(map.layout().look, NIGHT, "{map:?} keeps the night until its own rollout step");
        }
        assert_eq!(MapId::Dam.layout().look, DAM_NIGHT);
        assert_eq!(DAM_NIGHT.ambient(true), 0.0);
    }

    #[test]
    fn dusk_casts_two_cascades_out_to_the_fog() {
        let config = DUSK.cascade_config();
        assert_eq!(config.bounds.len(), 2);
        assert!((config.bounds[1] - 45.0).abs() < 1e-3);
    }
}
