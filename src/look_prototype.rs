//! PROTOTYPE, throwaway (wayfinder ticket #277 "How should the Greybox be lit?"). Not for main.
//!
//! Four lighting and post looks for the flat-coloured Greybox, switchable in a running game:
//!
//! - `today`:   the game as it ships, the baseline.
//! - `moonlit`: the research's "Moody A". A cold, strong moon with volumetric shafts through a
//!   ground fog, lamps as warm fill, the 4 lamps nearest the eye as shadowed spots. AgX, Bloom,
//!   SSAO, SMAA.
//! - `dusk`:    an overcast dusk. A low, warm, shadowed sun and a high, cool sky ambient; no
//!   volumetrics; lamps barely on. Readable, grim, daylight-ish.
//! - `lamplit`: pitch dark, lit only by the lamps. Every lamp a downward spot with a glowing bulb,
//!   the 4 nearest shadowed and casting volumetric cones into a thin haze. Strong Bloom, warm grade.
//!
//! `ZOMBIES_LOOK=<name>` picks the look at start (default `today`); `]` and `[` cycle it in game.
//! `ZOMBIES_LOOK_TOUR=<dir>` skips the menu, starts a Greybox Run, visits fixed viewpoints, saves a
//! screenshot of every look at each into `<dir>`, and quits. It works with `ZOMBIES_BENCH` too: the
//! benchmark then measures whichever look `ZOMBIES_LOOK` names.

use bevy::anti_alias::smaa::Smaa;
use bevy::camera::Hdr;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{CascadeShadowConfig, CascadeShadowConfigBuilder, FogVolume, VolumetricFog, VolumetricLight};
use bevy::pbr::{DistanceFog, ScreenSpaceAmbientOcclusion};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::view::ColorGrading;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

use crate::maps::map::{MapId, palette};
use crate::maps::zone::ZoneId;
use crate::players::player::{Health, LocalPlayer, Player, PlayerCamera, Teleport};
use crate::run::mode::Mode;
use crate::state::GameState;

/// The map's moon (or sun): its one directional light.
#[derive(Component)]
pub struct Moon;

/// One of the map's lamps, in the Zone it lights.
#[derive(Component)]
pub struct Lamp(pub ZoneId);

#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Look {
    Today,
    Moonlit,
    Dusk,
    Lamplit,
}

const LOOKS: [Look; 4] = [Look::Today, Look::Moonlit, Look::Dusk, Look::Lamplit];

impl Look {
    fn name(self) -> &'static str {
        match self {
            Look::Today => "today",
            Look::Moonlit => "moonlit",
            Look::Dusk => "dusk",
            Look::Lamplit => "lamplit",
        }
    }

    pub fn from_env() -> Self {
        let Ok(name) = std::env::var("ZOMBIES_LOOK") else { return Look::Today };
        LOOKS
            .into_iter()
            .find(|look| look.name() == name.to_ascii_lowercase())
            .unwrap_or_else(|| panic!("ZOMBIES_LOOK={name}: expected today, moonlit, dusk or lamplit"))
    }

    fn step(self, by: i32) -> Self {
        let at = LOOKS.iter().position(|&look| look == self).unwrap() as i32;
        LOOKS[(at + by).rem_euclid(LOOKS.len() as i32) as usize]
    }
}

/// How a lamp is lit: as an omni point or a downward spot, with or without a shadow and a
/// volumetric cone.
#[derive(Component, Clone, Copy, PartialEq, Debug)]
struct LampKind {
    spot: bool,
    shadow: bool,
    volumetric: bool,
}

/// The lamp's glowing bulb.
#[derive(Component)]
struct Bulb;

/// The fog volume the volumetric looks light up.
#[derive(Component)]
struct Haze;

/// How many lamps nearest the eye may cast shadows: the research's budget of about 4 per view.
const SHADOWED_LAMPS: usize = 4;

pub struct LookPrototypePlugin;

impl Plugin for LookPrototypePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Look::from_env())
            .add_systems(Update, (cycle, apply_camera, apply_world, apply_lamps).chain().run_if(in_state(GameState::Playing)))
            .add_systems(Update, add_bulbs.run_if(in_state(GameState::Playing)));
        if let Ok(dir) = std::env::var("ZOMBIES_LOOK_TOUR") {
            std::fs::create_dir_all(&dir).unwrap();
            app.insert_resource(Tour { dir, ..default() })
                .add_systems(OnEnter(GameState::MainMenu), start_run)
                .add_systems(Update, (keep_unhurt, tour).run_if(in_state(GameState::Playing)));
        }
    }
}

fn cycle(keys: Res<ButtonInput<KeyCode>>, mut look: ResMut<Look>) {
    let by = if keys.just_pressed(KeyCode::BracketRight) {
        1
    } else if keys.just_pressed(KeyCode::BracketLeft) {
        -1
    } else {
        return;
    };
    *look = look.step(by);
    info!("look: {}", look.name());
}

/// The camera's post stack.
fn apply_camera(
    mut commands: Commands,
    look: Res<Look>,
    cameras: Query<Entity, With<PlayerCamera>>,
    added: Query<(), Added<PlayerCamera>>,
    mut grading: Query<(&mut ColorGrading, &mut DistanceFog), With<PlayerCamera>>,
) {
    if !look.is_changed() && added.is_empty() {
        return;
    }
    for camera in &cameras {
        let mut camera = commands.entity(camera);
        camera.remove::<(Smaa, ScreenSpaceAmbientOcclusion, Bloom, Hdr, VolumetricFog)>();
        camera.insert((Msaa::Sample4, Tonemapping::TonyMcMapface));
        match *look {
            Look::Today => {}
            Look::Moonlit => {
                camera.insert((
                    Msaa::Off,
                    Smaa::default(),
                    ScreenSpaceAmbientOcclusion::default(),
                    Hdr,
                    Bloom::NATURAL,
                    Tonemapping::AgX,
                    VolumetricFog {
                        ambient_intensity: 0.0,
                        step_count: 16,
                        ..default()
                    },
                ));
            }
            Look::Dusk => {
                camera.insert((
                    Msaa::Off,
                    Smaa::default(),
                    ScreenSpaceAmbientOcclusion::default(),
                    Hdr,
                    Bloom {
                        intensity: 0.05,
                        ..Bloom::NATURAL
                    },
                    Tonemapping::TonyMcMapface,
                ));
            }
            Look::Lamplit => {
                camera.insert((
                    Msaa::Off,
                    Smaa::default(),
                    ScreenSpaceAmbientOcclusion::default(),
                    Hdr,
                    Bloom {
                        intensity: 0.3,
                        ..Bloom::OLD_SCHOOL
                    },
                    Tonemapping::AgX,
                    VolumetricFog {
                        ambient_intensity: 0.0,
                        step_count: 16,
                        ..default()
                    },
                ));
            }
        }
    }
    for (mut grading, mut fog) in &mut grading {
        // The Teleporter owns post_saturation; the look takes the rest of the global grade.
        let saturation = grading.global.post_saturation;
        grading.global = default();
        grading.global.post_saturation = saturation;
        let (fog_colour, exposure, temperature, tint) = match *look {
            Look::Today => (Color::srgb(0.02, 0.02, 0.04), 0.0, 0.0, 0.0),
            Look::Moonlit => (Color::srgb(0.03, 0.04, 0.07), 0.3, -0.1, 0.0),
            Look::Dusk => (Color::srgb(0.28, 0.25, 0.28), 0.0, 0.05, 0.02),
            Look::Lamplit => (Color::srgb(0.0, 0.0, 0.0), 0.2, 0.15, 0.0),
        };
        grading.global.exposure = exposure;
        grading.global.temperature = temperature;
        grading.global.tint = tint;
        fog.color = fog_colour;
    }
}

/// The moon, the ambient, the sky's colour and the haze.
fn apply_world(
    mut commands: Commands,
    look: Res<Look>,
    map: Res<MapId>,
    mut moons: Query<(Entity, &mut DirectionalLight, &mut Transform), With<Moon>>,
    added: Query<(), Added<Moon>>,
    haze: Query<Entity, With<Haze>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut clear: ResMut<ClearColor>,
) {
    if !look.is_changed() && added.is_empty() {
        return;
    }
    let sun = map.layout().sun;
    // (illuminance, colour, direction, ambient colour, ambient brightness, sky)
    let (lux, colour, direction, ambient_colour, ambient_brightness, sky) = match *look {
        Look::Today => (800.0, Color::srgb(0.7, 0.75, 1.0), sun, Color::srgb(0.6, 0.65, 0.8), 120.0, Color::srgb(0.02, 0.02, 0.04)),
        Look::Moonlit => (2500.0, Color::srgb(0.6, 0.7, 1.0), sun, Color::srgb(0.45, 0.55, 0.85), 60.0, Color::srgb(0.03, 0.04, 0.07)),
        Look::Dusk => (
            4000.0,
            Color::srgb(1.0, 0.62, 0.38),
            Vec3::new(sun.x, sun.y * 0.2, sun.z),
            Color::srgb(0.55, 0.6, 0.75),
            450.0,
            Color::srgb(0.28, 0.25, 0.28),
        ),
        Look::Lamplit => (60.0, Color::srgb(0.6, 0.7, 1.0), sun, Color::srgb(0.5, 0.55, 0.7), 8.0, Color::srgb(0.0, 0.0, 0.0)),
    };
    ambient.color = ambient_colour;
    ambient.brightness = ambient_brightness;
    clear.0 = sky;
    for (moon, mut light, mut transform) in &mut moons {
        light.illuminance = lux;
        light.color = colour;
        light.shadow_maps_enabled = true;
        *transform = Transform::default().looking_at(direction, Vec3::Y);
        let mut moon = commands.entity(moon);
        if *look == Look::Today {
            moon.insert(CascadeShadowConfig::from(CascadeShadowConfigBuilder::default()));
        } else {
            // 2 cascades out to the fog's end, as the research measured.
            moon.insert(CascadeShadowConfig::from(CascadeShadowConfigBuilder {
                num_cascades: 2,
                maximum_distance: 45.0,
                first_cascade_far_bound: 12.0,
                ..default()
            }));
        }
        if *look == Look::Moonlit {
            moon.insert(VolumetricLight);
        } else {
            moon.remove::<VolumetricLight>();
        }
    }
    for haze in &haze {
        commands.entity(haze).despawn();
    }
    let density = match *look {
        Look::Moonlit => Some(0.04),
        Look::Lamplit => Some(0.005),
        _ => None,
    };
    if let Some(density_factor) = density {
        let (lo, hi) = map.layout().plan;
        let centre = (lo + hi) / 2.0;
        let size = hi - lo;
        commands.spawn((
            Haze,
            crate::state::InRun,
            FogVolume {
                density_factor,
                ..default()
            },
            Transform::from_xyz(centre.x, 4.0, centre.y).with_scale(Vec3::new(size.x, 10.0, size.y)),
        ));
    }
}

/// Each lamp's kind, its brightness and its bulb, with shadows on the few nearest the eye.
fn apply_lamps(
    mut commands: Commands,
    look: Res<Look>,
    eye: Query<&GlobalTransform, With<PlayerCamera>>,
    lamps: Query<(Entity, &Lamp, &Transform, Option<&LampKind>)>,
    mut bulbs: Query<(&ChildOf, &mut Visibility), With<Bulb>>,
) {
    let Some(eye) = eye.iter().next().map(GlobalTransform::translation) else { return };
    let mut by_distance: Vec<_> = lamps.iter().collect();
    by_distance.sort_by(|a, b| a.2.translation.distance_squared(eye).total_cmp(&b.2.translation.distance_squared(eye)));
    for (rank, &(entity, lamp, transform, kind)) in by_distance.iter().enumerate() {
        let near = rank < SHADOWED_LAMPS;
        let want = match *look {
            Look::Today | Look::Dusk => LampKind {
                spot: false,
                shadow: false,
                volumetric: false,
            },
            Look::Moonlit => LampKind {
                spot: near,
                shadow: near,
                volumetric: false,
            },
            Look::Lamplit => LampKind {
                spot: true,
                shadow: near,
                volumetric: near,
            },
        };
        if kind == Some(&want) && !look.is_changed() {
            continue;
        }
        let colour = palette(Some(lamp.0)).light;
        let intensity = match *look {
            Look::Today => 400_000.0,
            Look::Moonlit => 250_000.0,
            Look::Dusk => 60_000.0,
            Look::Lamplit => 900_000.0,
        };
        let mut lamp = commands.entity(entity);
        lamp.remove::<(PointLight, SpotLight, VolumetricLight)>();
        if want.spot {
            lamp.insert((
                SpotLight {
                    color: colour,
                    intensity,
                    range: 25.0,
                    outer_angle: 1.25,
                    inner_angle: 0.5,
                    shadow_maps_enabled: want.shadow,
                    ..default()
                },
                Transform::from_translation(transform.translation).looking_at(transform.translation - Vec3::Y + Vec3::Z * 0.001, Vec3::Y),
            ));
        } else {
            lamp.insert((
                PointLight {
                    color: colour,
                    intensity,
                    range: 25.0,
                    shadow_maps_enabled: want.shadow,
                    ..default()
                },
                Transform::from_translation(transform.translation),
            ));
        }
        if want.volumetric {
            lamp.insert(VolumetricLight);
        }
        lamp.insert(want);
    }
    if look.is_changed() {
        let shown = matches!(*look, Look::Moonlit | Look::Lamplit);
        for (_, mut visibility) in &mut bulbs {
            *visibility = if shown { Visibility::Inherited } else { Visibility::Hidden };
        }
    }
}

fn add_bulbs(
    mut commands: Commands,
    look: Res<Look>,
    lamps: Query<(Entity, &Lamp), Added<Lamp>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if lamps.is_empty() {
        return;
    }
    let mesh = meshes.add(Sphere::new(0.12));
    let shown = matches!(*look, Look::Moonlit | Look::Lamplit);
    for (entity, lamp) in &lamps {
        let material = materials.add(StandardMaterial {
            base_color: Color::BLACK,
            emissive: LinearRgba::from(palette(Some(lamp.0)).light) * 40.0,
            ..default()
        });
        commands.entity(entity).with_child((
            Bulb,
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material),
            Transform::default(),
            if shown { Visibility::Inherited } else { Visibility::Hidden },
        ));
    }
}

// --- The screenshot tour ---------------------------------------------------------------------

#[derive(Resource, Default)]
struct Tour {
    dir: String,
    frames: u32,
    view: usize,
    look: usize,
    /// Frames into the current shot.
    settle: u32,
    started: bool,
}

/// (name, feet, looking at, pitch)
const VIEWS: [(&str, Vec3, Vec3, f32); 5] = [
    ("start-room", Vec3::new(0.0, 0.0, 25.0), Vec3::new(0.0, 0.0, 14.0), -0.05),
    ("yard", Vec3::new(10.0, 0.0, 11.0), Vec3::new(-8.0, 0.0, -10.0), 0.05),
    ("west-hall", Vec3::new(-20.0, 0.0, 24.0), Vec3::new(-20.0, 0.0, -10.0), 0.12),
    ("dining-room", Vec3::new(-24.0, 0.0, -16.0), Vec3::new(-2.0, 0.0, -24.0), -0.05),
    ("workshop", Vec3::new(-32.0, 0.0, -40.0), Vec3::new(-6.0, 0.0, -48.0), -0.05),
];

/// Frames to wait before the tour starts, for the Map to finish building and pipelines to compile.
const TOUR_WARM_UP: u32 = 240;

fn start_run(mut map: ResMut<MapId>, mut mode: ResMut<Mode>, mut next: ResMut<NextState<GameState>>) {
    if std::env::var("ZOMBIES_BENCH").is_ok() {
        return;
    }
    *map = MapId::Greybox;
    *mode = Mode::Classic;
    next.set(GameState::Playing);
}

fn keep_unhurt(mut players: Query<&mut Health, With<Player>>) {
    for mut health in &mut players {
        health.current = health.max;
    }
}

fn tour(
    mut commands: Commands,
    mut tour: ResMut<Tour>,
    mut look: ResMut<Look>,
    player: Query<Entity, With<LocalPlayer>>,
    mut camera: Query<&mut Transform, (With<PlayerCamera>, Without<LocalPlayer>)>,
    mut body: Query<&mut Transform, (With<LocalPlayer>, Without<PlayerCamera>)>,
    mut teleports: MessageWriter<Teleport>,
    mut exit: MessageWriter<AppExit>,
) {
    tour.frames += 1;
    if tour.frames < TOUR_WARM_UP {
        return;
    }
    if tour.view == VIEWS.len() {
        // A few frames for the last screenshot to reach the disk.
        tour.settle += 1;
        if tour.settle > 30 {
            exit.write(AppExit::Success);
        }
        return;
    }
    let Ok(player) = player.single() else { return };
    let (name, feet, at, pitch) = VIEWS[tour.view];
    let facing = Transform::from_translation(feet).looking_at(Vec3::new(at.x, feet.y, at.z), Vec3::Y).rotation;
    for mut transform in &mut body {
        transform.rotation = facing;
    }
    if tour.settle == 0 {
        teleports.write(Teleport {
            player,
            feet,
            facing: Some(facing),
        });
        *look = LOOKS[tour.look];
    }
    for mut transform in &mut camera {
        transform.rotation = Quat::from_rotation_x(pitch);
    }
    tour.settle += 1;
    // The first pass through every look compiles its pipelines; give it longer.
    let wait = if tour.started { 40 } else { 150 };
    if tour.settle < wait {
        return;
    }
    let path = format!("{}/{}-{}.png", tour.dir, name, LOOKS[tour.look].name());
    info!("tour: {path}");
    commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    tour.settle = 0;
    tour.look += 1;
    if tour.look == LOOKS.len() {
        tour.look = 0;
        tour.view += 1;
        tour.started = true;
    }
}
