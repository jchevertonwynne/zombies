//! How each Map is lit and graded: its `Look` (in its `Layout`), over one post stack shared by
//! every Map. The sun, the ambient, the sky and fog colour and how strong the lamps are come from
//! the Look; the Player camera gets HDR, SMAA (MSAA off), SSAO, a faint Bloom, TonyMcMapface and
//! the Look's grade. A Map with Darkness keeps its own close black fog and stronger Bloom.
//!
//! A Look may name a CC0 HDRI sky (a KTX2 cubemap from `texpack --sky`), which is the Player
//! camera's skybox and, through the `diffuse.ktx2` and `specular.ktx2` prefiltered beside it, its
//! outdoor ambient and reflections (Bevy's `GeneratedEnvironmentMapLight` filtered the sky on the
//! GPU every frame, which cost the Greybox ~4 ms). If the Look names none, or the file isn't on
//! disk, the sky colour is the clear colour; if the prefiltered maps aren't, the sky lights nothing.

use bevy::anti_alias::smaa::Smaa;
use bevy::camera::Hdr;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{CascadeShadowConfig, CascadeShadowConfigBuilder, EnvironmentMapLight, Skybox};
use bevy::pbr::{DistanceFog, FogFalloff, ScreenSpaceAmbientOcclusion};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::view::ColorGrading;

use crate::maps::darkness::Lamp;
use crate::maps::map::{Layout, MapId};
use crate::maps::zone::{PlayerZone, ZoneId};
use crate::maps::map::textures::on_disk;
use crate::players::player::{LocalPlayer, PlayerCamera};
use crate::state::GameState;
use crate::tuning::{AMBIENT_BRIGHTNESS, FOG_COLOUR_RATE, FOG_END, FOG_START, SHADOWED_LAMPS};

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
    /// The distance fog's linear falloff, (start, end), that the fog rests at and that the Fog
    /// Twist and the Hellhound fog restore to.
    pub fog_range: (f32, f32),
    /// The fog's colour while the eye is indoors on a roofed Map.
    pub indoor_fog: Color,
    /// The grade's saturation, 1 as it comes (`ColorGrading.global.post_saturation`).
    pub saturation: f32,
    /// How far each indoor Zone's light probe is mixed from `INDOOR_PROBE_COLOUR` towards the
    /// Zone's lamp colour: 0 keeps the constant probe.
    pub probe_tint: f32,
    /// The colour of the Map's Still water, in place of the one its depth gives it: `None` keeps
    /// the shallows' and the deep's own. The surface stays glossy either way.
    pub water: Option<Color>,
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
    fog_range: (FOG_START, FOG_END),
    indoor_fog: INDOOR_FOG,
    saturation: 1.0,
    probe_tint: 0.0,
    water: None,
};

/// The Dam (#379): the night, under a dark, starry CC0 sky seen from the Crest Road. Its ambient
/// is the Darkness's none, and the sky lights nothing, so only the lamps and the moon do.
pub const DAM_NIGHT: Look = Look {
    // Poly Haven's Qwantani Night (Pure Sky), a deep blue-black with a faint glow low down.
    sky_map: Some(("skies/qwantani_night_puresky/sky.ktx2", 180.0)),
    ..NIGHT
};

/// The Village (#331): a moonlit night. A cool, high moon with soft cascaded shadows, over the
/// same CC0 night sky as the Dam, which here also lights the streets faintly through its
/// prefiltered maps; the gas lamps do the rest. Cool fog the colour of the sky's horizon, and a
/// slightly cool grade so the lamps' warmth stands out against it.
pub const VILLAGE_NIGHT: Look = Look {
    sun_colour: Color::srgb(0.62, 0.72, 1.0),
    sun_lux: 1600.0,
    sun_height: 0.8,
    cascades: Some((3, 70.0)),
    ambient_colour: Color::srgb(0.5, 0.6, 0.85),
    ambient_brightness: 70.0,
    sky: Color::srgb(0.01, 0.015, 0.035),
    fog: Color::srgb(0.025, 0.035, 0.07),
    sky_map: Some(("skies/qwantani_night_puresky/sky.ktx2", 200.0)),
    lamps: 1.0,
    temperature: -0.06,
    tint: 0.0,    fog_range: (FOG_START, FOG_END),
    indoor_fog: INDOOR_FOG,
    saturation: 1.0,
    probe_tint: 0.0,
    water: None,
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
    tint: 0.02,    fog_range: (FOG_START, FOG_END),
    indoor_fog: INDOOR_FOG,
    saturation: 1.0,
    probe_tint: 0.0,
    water: None,
};

/// The Cove (#380): golden hour over the sea. The sun hangs low in the south-west, big and warm, so
/// the town's whitewash and ochre glow and the lanes fall in long shadows; the fog is a pale warm
/// haze the sea fades into, never black, under Poly Haven's Evening Road 01 (Pure Sky).
pub const GOLDEN_HOUR: Look = Look {
    sun_colour: Color::srgb(1.0, 0.74, 0.46),
    sun_lux: 6500.0,
    sun_height: 0.12,
    cascades: Some((3, 90.0)),
    ambient_colour: Color::srgb(0.62, 0.7, 0.88),
    ambient_brightness: 550.0,
    sky: Color::srgb(0.5, 0.52, 0.62),
    fog: Color::srgb(0.68, 0.55, 0.5),
    // Turned to put its sun where the Layout's is.
    sky_map: Some(("skies/evening_road_01_puresky/sky.ktx2", 300.0)),
    lamps: 1.0,
    temperature: 0.06,
    tint: 0.0,    fog_range: (FOG_START, FOG_END),
    indoor_fog: INDOOR_FOG,
    saturation: 1.0,
    probe_tint: 0.0,
    water: None,
};

/// The Quarry (#330): an overcast late afternoon. A soft, pale, high sun through cloud, a flat cold
/// grey-green ambient from the sky, grey-green haze over the pit, a faintly green grade (a negative tint is green), and
/// the lamps (the sheds') the only warm light.
pub const OVERCAST: Look = Look {
    sun_colour: Color::srgb(0.84, 0.9, 0.86),
    sun_lux: 3500.0,
    sun_height: 1.0,
    cascades: Some((3, 70.0)),
    ambient_colour: Color::srgb(0.68, 0.74, 0.7),
    ambient_brightness: 340.0,
    sky: Color::srgb(0.48, 0.52, 0.48),
    fog: Color::srgb(0.43, 0.47, 0.45),
    // Poly Haven's Kloofendal Overcast (Pure Sky): a low-contrast cold grey cloud deck.
    sky_map: Some(("skies/kloofendal_overcast_puresky/sky.ktx2", 300.0)),
    lamps: 1.0,
    temperature: 0.0,
    tint: -0.025,    fog_range: (FOG_START, FOG_END),
    indoor_fog: INDOOR_FOG,
    saturation: 1.0,
    probe_tint: 0.0,
    water: None,
};

/// The Polder (#372): a northern dusk. A low, cool, weak sun behind thick cloud, a flat blue-grey
/// ambient from the overcast sky, a grey-green fog the canals fade into, a little warm lamplight.
pub const NORTHERN_DUSK: Look = Look {
    sun_colour: Color::srgb(0.95, 0.78, 0.66),
    sun_lux: 1800.0,
    sun_height: 0.25,
    cascades: Some((2, 45.0)),
    ambient_colour: Color::srgb(0.6, 0.66, 0.74),
    ambient_brightness: 260.0,
    sky: Color::srgb(0.3, 0.33, 0.36),
    fog: Color::srgb(0.27, 0.3, 0.33),
    // A CC0 overcast sky from Poly Haven (see `assets/manifest.json`).
    sky_map: Some(("skies/overcast_soil_puresky/sky.ktx2", 200.0)),
    lamps: 1.0,
    temperature: -0.03,
    tint: 0.01,    fog_range: (FOG_START, FOG_END),
    indoor_fog: INDOOR_FOG,
    saturation: 1.0,
    probe_tint: 0.0,
    water: None,
};

/// The Sanatorium (#522): a fogbound hill. A high, diffuse, cold green-grey light through a thick
/// pale fog that swallows the hill and the far ends of the corridors, washed out (saturation 0.8)
/// and graded cold and green, with the lamps (fluorescent tubes) a little under full. Indoors the
/// fog is a green-black, and each Zone's probe is half-mixed towards its lamp colour. It reuses
/// the Polder's overcast sky, which the fog hides most of.
pub const FOGBOUND: Look = Look {
    sun_colour: Color::srgb(0.8, 0.86, 0.84),
    sun_lux: 900.0,
    sun_height: 0.9,
    cascades: Some((2, 35.0)),
    ambient_colour: Color::srgb(0.6, 0.7, 0.64),
    ambient_brightness: 220.0,
    sky: Color::srgb(0.5, 0.55, 0.52),
    fog: Color::srgb(0.52, 0.57, 0.53),
    sky_map: Some(("skies/overcast_soil_puresky/sky.ktx2", 140.0)),
    lamps: 0.85,
    temperature: -0.07,
    tint: -0.045,
    fog_range: (5.0, 34.0),
    indoor_fog: Color::srgb(0.012, 0.02, 0.016),
    saturation: 0.8,
    probe_tint: 0.5,
    water: None,
};

/// The Archive (#535): lamplight, with no sky and no sun. Everything is underground, so the sun is
/// 0 lux (and casts no shadows, so no cascades are rendered) and the sky shows nowhere; the daylight
/// is faked by emissive panels, cool spots and additive shafts (`archive::daylight`). A warm amber
/// ambient and grade (the warmest of the Maps), faded-paper saturation, and a lamp-lit dust-brown
/// fog the far end of a 40 m hall fades into.
pub const LAMPLIGHT: Look = Look {
    sun_colour: Color::srgb(1.0, 0.85, 0.65),
    sun_lux: 0.0,
    sun_height: 1.0,
    cascades: None,
    ambient_colour: Color::srgb(1.0, 0.8, 0.58),
    ambient_brightness: 110.0,
    sky: Color::srgb(0.012, 0.01, 0.008),
    fog: Color::srgb(0.012, 0.01, 0.008),
    sky_map: None,
    lamps: 0.9,
    temperature: 0.08,
    tint: 0.015,
    fog_range: (6.0, 30.0),
    indoor_fog: Color::srgb(0.05, 0.038, 0.026),
    saturation: 0.9,
    probe_tint: 0.6,
    // Dark tea, over a bed it hides (#539).
    water: Some(Color::srgba(0.09, 0.075, 0.05, 0.8)),
};

/// The Refinery (#565): a night lit from below. A cool, low moon over a warm, smoky ambient, a
/// brown-black sky and a smoky brown fog the 70 m down the rack fades into, graded warm and a
/// little over-saturated (the only Look above 1) so hot orange stands out on black steel. The flare
/// stack (`refinery::sky`) is the key light; it reuses the Dam's night sky at half strength, which
/// the smoke dims. Indoors the fog is a warm black, and each shed's probe leans 40% to its lamp.
pub const FLARE_NIGHT: Look = Look {
    sun_colour: Color::srgb(0.55, 0.65, 1.0),
    sun_lux: 900.0,
    sun_height: 0.7,
    cascades: Some((2, 50.0)),
    ambient_colour: Color::srgb(0.75, 0.55, 0.42),
    ambient_brightness: 55.0,
    sky: Color::srgb(0.035, 0.018, 0.012),
    fog: Color::srgb(0.09, 0.05, 0.03),
    sky_map: Some(("skies/qwantani_night_puresky/sky.ktx2", 90.0)),
    lamps: 1.0,
    temperature: 0.03,
    tint: 0.0,
    fog_range: (18.0, 130.0),
    indoor_fog: Color::srgb(0.03, 0.022, 0.016),
    saturation: 1.1,
    probe_tint: 0.4,
    water: None,
};

/// How strongly the sky lights the Map, as a fraction of how bright it shows.
const SKY_LIGHT: f32 = 0.7;

/// The fog's colour while the eye is indoors on a roofed Map: near-black, so a room doesn't glow
/// the sky's colour.
pub const INDOOR_FOG: Color = Color::srgb(0.02, 0.018, 0.018);

/// The colour of each indoor Zone's constant light probe, before a Look's `probe_tint` mixes it
/// towards the Zone's lamp colour.
pub const INDOOR_PROBE_COLOUR: Color = Color::srgb(0.75, 0.7, 0.65);

/// The shared Bloom: just enough for bulbs and signs to glow.
const BLOOM: f32 = 0.05;

impl Look {
    /// Whether the sun casts shadows: not a sunless Look's (no cascades are rendered for it).
    pub fn casts_sun_shadows(&self) -> bool {
        self.sun_lux > 0.0
    }

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

    /// The colour of a Zone's light probe, given its lamps' colour: `INDOOR_PROBE_COLOUR` mixed
    /// `probe_tint` of the way towards it (exactly the constant at 0).
    pub fn probe_colour(&self, lamp: Color) -> Color {
        if self.probe_tint == 0.0 {
            return INDOOR_PROBE_COLOUR;
        }
        LinearRgba::from(INDOOR_PROBE_COLOUR).mix(&LinearRgba::from(lamp), self.probe_tint).into()
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
            .add_systems(Update, (follow_zone_fog, shadow_nearest_lamps, crate::maps::map::archive::daylight::drift_motes, crate::maps::map::refinery::sky::sway_flames, crate::maps::map::refinery::sky::drift_plume, crate::maps::map::refinery::sky::flicker_flare).run_if(in_state(GameState::Playing)));
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
    if layout.roofed && !layout.darkness && layout.indoor(zone) { layout.look.indoor_fog } else { layout.look.fog }
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

/// Only the spot lamps nearest the eye cast shadows (never a floodlight, which keeps none).
pub(crate) fn shadow_nearest_lamps(eyes: Query<&GlobalTransform, With<PlayerCamera>>, mut lamps: Query<(&GlobalTransform, &mut SpotLight), (With<Lamp>, Without<crate::maps::darkness::flood::FloodLight>)>) {
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

/// The file named `name` in the same directory as the asset `path`.
fn sibling(path: &str, name: &str) -> String {
    std::path::Path::new(path).with_file_name(name).to_string_lossy().into_owned()
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
        // Under Darkness the sky is only to look at: it lights nothing. Otherwise it lights the Map
        // through the maps `texpack --sky` prefiltered beside it, if they're on disk.
        let (diffuse, specular) = (sibling(file, "diffuse.ktx2"), sibling(file, "specular.ktx2"));
        if !layout.darkness && on_disk(&diffuse) && on_disk(&specular) {
            camera.insert(EnvironmentMapLight {
                diffuse_map: asset_server.load(diffuse),
                specular_map: asset_server.load(specular),
                intensity: brightness * SKY_LIGHT,
                ..default()
            });
        }
    }
    if layout.darkness {
        // Darkness brings its own Bloom and fog.
        return;
    }
    if let Ok((mut grading, mut fog)) = cameras.get_mut(add.entity) {
        grading.global.temperature = look.temperature;
        grading.global.tint = look.tint;
        grading.global.post_saturation = look.saturation;
        fog.falloff = FogFalloff::Linear { start: look.fog_range.0, end: look.fog_range.1 };
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
        assert_eq!(MapId::Village.layout().look, VILLAGE_NIGHT);
        assert_eq!(MapId::Cove.layout().look, GOLDEN_HOUR);
        assert_eq!(MapId::Quarry.layout().look, OVERCAST);
        assert_eq!(MapId::Dam.layout().look, DAM_NIGHT);
        assert_eq!(DAM_NIGHT.ambient(true), 0.0);
    }

    /// Every Map but the Sanatorium keeps what its Look did before the four new fields.
    #[test]
    fn other_maps_looks_unchanged_by_the_new_fields() {
        for map in MapId::ALL.into_iter().filter(|&map| !matches!(map, MapId::Sanatorium | MapId::Archive | MapId::Refinery)) {
            let look = map.layout().look;
            assert_eq!(look.fog_range, (FOG_START, FOG_END), "{map:?}");
            assert_eq!(look.indoor_fog, INDOOR_FOG, "{map:?}");
            assert_eq!(look.saturation, 1.0, "{map:?}");
            assert_eq!(look.probe_tint, 0.0, "{map:?}");
            assert_eq!(look.probe_colour(Color::srgb(0.1, 0.9, 0.2)), INDOOR_PROBE_COLOUR, "{map:?}");
            for &zone in map.layout().zones {
                assert_eq!(fog_colour(map.layout(), zone), if map.layout().roofed && !map.layout().darkness && map.layout().indoor(zone) { INDOOR_FOG } else { look.fog });
            }
        }
        assert_eq!(MapId::Sanatorium.layout().look, FOGBOUND);
        assert_eq!(MapId::Archive.layout().look, LAMPLIGHT);
        assert_eq!(MapId::Refinery.layout().look, FLARE_NIGHT);
        assert_eq!(FOGBOUND.fog_range, (5.0, 34.0));
        assert_eq!(FOGBOUND.saturation, 0.8);
        // Every other Map's Look is the one it had, not the Refinery's.
        for map in MapId::ALL.into_iter().filter(|&map| map != MapId::Refinery) {
            assert_ne!(map.layout().look, FLARE_NIGHT, "{map:?}");
        }
    }

    /// The Refinery's Look is #510's table, and its Layout's moon stands where the ticket put it.
    #[test]
    fn the_refinerys_look_is_flare_night() {
        let look = MapId::Refinery.layout().look;
        assert_eq!(look, FLARE_NIGHT);
        assert_eq!((look.sun_lux, look.sun_height, look.cascades), (900.0, 0.7, Some((2, 50.0))));
        assert_eq!((look.ambient_brightness, look.fog_range, look.saturation, look.probe_tint), (55.0, (18.0, 130.0), 1.1, 0.4));
        assert_eq!((look.lamps, look.temperature, look.tint, look.water), (1.0, 0.03, 0.0, None));
        assert_eq!(look.sky_map, Some(("skies/qwantani_night_puresky/sky.ktx2", 90.0)));
        assert_eq!(MapId::Refinery.layout().sun, Vec3::new(0.4, -1.0, -0.6));
        assert!(look.casts_sun_shadows());
        // The only Look above full saturation; the fog is brown, the moon cold over a warm ambient.
        for map in MapId::ALL.into_iter().filter(|&map| map != MapId::Refinery) {
            assert!(map.layout().look.saturation <= 1.0, "{map:?}");
        }
        let (fog, moon, ambient) = (look.fog.to_srgba(), look.sun_colour.to_srgba(), look.ambient_colour.to_srgba());
        assert!(fog.red > fog.green && fog.green > fog.blue);
        assert!(moon.blue > moon.red && ambient.red > ambient.blue);
    }

    /// The sunless shadow rule touches only the Look with no sun: every other Look, on every other
    /// Map, still casts its sun's shadows (and keeps the cascades it had).
    #[test]
    fn other_maps_looks_unchanged_by_the_sunless_shadow_rule() {
        for map in MapId::ALL.into_iter().filter(|&map| map != MapId::Archive) {
            let look = map.layout().look;
            assert!(look.sun_lux > 0.0, "{map:?}");
            assert!(look.casts_sun_shadows(), "{map:?}");
        }
        assert_eq!(MapId::Polder.layout().look, NORTHERN_DUSK);
        assert_eq!(MapId::Cove.layout().look, GOLDEN_HOUR);
        assert_eq!(MapId::Sanatorium.layout().look.sun_lux, 900.0);
        assert!(!LAMPLIGHT.casts_sun_shadows());
        assert_eq!(LAMPLIGHT.sun_lux, 0.0);
        assert!(LAMPLIGHT.sky_map.is_none());
        assert_eq!((LAMPLIGHT.fog_range, LAMPLIGHT.lamps, LAMPLIGHT.saturation, LAMPLIGHT.probe_tint), ((6.0, 30.0), 0.9, 0.9, 0.6));
    }

    /// Only the Archive's Look has a water colour of its own: every other Map's water keeps the
    /// colours its depth gives it, whichever Look it's drawn under.
    #[test]
    fn other_maps_looks_unchanged_by_water() {
        for map in MapId::ALL.into_iter().filter(|&map| map != MapId::Archive) {
            let look = map.layout().look;
            assert_eq!(look.water, None, "{map:?}");
            assert_eq!(look, Look { water: None, ..look }, "{map:?}");
        }
        for look in [NIGHT, DAM_NIGHT, VILLAGE_NIGHT, DUSK, GOLDEN_HOUR, OVERCAST, NORTHERN_DUSK, FOGBOUND] {
            assert_eq!(look.water, None);
        }
        assert_eq!(LAMPLIGHT.water, Some(Color::srgba(0.09, 0.075, 0.05, 0.8)));
        assert_eq!(MapId::Archive.layout().look.water, LAMPLIGHT.water);
        // The Cove's and the Polder's water is drawn in the colours it always was.
        let depth = Color::srgba(0.5, 0.78, 0.78, 0.55);
        assert_eq!(crate::maps::water::water_colour(&GOLDEN_HOUR, depth), depth);
        assert_eq!(crate::maps::water::water_colour(&NORTHERN_DUSK, depth), depth);
        assert_eq!(crate::maps::water::water_colour(&LAMPLIGHT, depth), LAMPLIGHT.water.unwrap());
    }

    #[test]
    fn the_sanatoriums_probes_lean_towards_their_lamps() {
        let lamp = Color::srgb(1.0, 0.66, 0.34);
        let (base, mixed) = (LinearRgba::from(INDOOR_PROBE_COLOUR), LinearRgba::from(FOGBOUND.probe_colour(lamp)));
        let lamp = LinearRgba::from(lamp);
        assert!((mixed.red - (base.red + lamp.red) / 2.0).abs() < 1e-5);
        assert!((mixed.blue - (base.blue + lamp.blue) / 2.0).abs() < 1e-5);
    }

    #[test]
    fn dusk_casts_two_cascades_out_to_the_fog() {
        let config = DUSK.cascade_config();
        assert_eq!(config.bounds.len(), 2);
        assert!((config.bounds[1] - 45.0).abs() < 1e-3);
    }
}
