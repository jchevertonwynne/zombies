use std::time::Duration;

use bevy::ecs::system::RunSystemOnce;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;

use super::*;
use crate::maps::generator::Generators;
use crate::maps::map::MapId;
use crate::state::GameState;

/// A frame of this many seconds.
const FRAME: f32 = 0.05;

/// `map` built and Playing, with its Generators all Unclaimed and Darkness running.
fn playing(map: MapId) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), TransformPlugin, StatesPlugin))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .insert_state(GameState::Playing)
        .insert_resource(map)
        .insert_resource(Generators::for_map(map))
        .insert_resource(GlobalAmbientLight::default())
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(FRAME)))
        .add_plugins(DarknessPlugin);
    app.world_mut().run_system_once(crate::maps::map::spawn_map).unwrap();
    app.update();
    app
}

/// Runs `secs` of frames.
fn run(app: &mut App, secs: f32) {
    for _ in 0..(secs / FRAME).round() as u32 {
        app.update();
    }
}

/// How bright `zone`'s lamps are, from 0 off to 1 full. Panics if they differ.
fn lamps(app: &mut App, zone: ZoneId) -> f32 {
    let mut query = app.world_mut().query::<(&Lamp, &PointLight)>();
    let levels: Vec<f32> = query
        .iter(app.world())
        .filter(|(lamp, _)| lamp.zone == zone)
        .map(|(_, light)| light.intensity / LAMP_INTENSITY)
        .collect();
    assert!(!levels.is_empty(), "{zone:?} has no lamps");
    assert!(levels.windows(2).all(|pair| (pair[0] - pair[1]).abs() < 1e-4), "{zone:?}'s lamps differ: {levels:?}");
    levels[0]
}

#[test]
fn on_the_dam_only_the_zones_outside_every_section_start_lit() {
    let mut app = playing(MapId::Dam);
    assert_eq!(lamps(&mut app, ZoneId::CrestRoad), 1.0);
    assert_eq!(lamps(&mut app, ZoneId::IntakeGallery), 0.0);
    assert_eq!(lamps(&mut app, ZoneId::TurbineHall), 0.0);
    assert_eq!(lamps(&mut app, ZoneId::ControlRoom), 0.0, "the Control Room waits for every Generator");
}

#[test]
fn a_map_without_darkness_keeps_every_lamp_on() {
    let mut app = playing(MapId::Greybox);
    for &(zone, _) in MapId::Greybox.layout().lamps {
        assert_eq!(lamps(&mut app, zone), 1.0);
    }
}

/// Generator `index` on `app`'s Map shown as `state`, as the Host says.
fn show(app: &mut App, index: usize, state: crate::maps::generator::GeneratorState) {
    app.world_mut().resource_mut::<Generators>().show(index, state);
}

const INTAKE: usize = 0;
const HELD: crate::maps::generator::GeneratorState = crate::maps::generator::GeneratorState::Held;
const UNCLAIMED: crate::maps::generator::GeneratorState = crate::maps::generator::GeneratorState::Unclaimed;

#[test]
fn a_powered_section_fades_up_over_about_a_second_and_a_bit() {
    let mut app = playing(MapId::Dam);
    show(&mut app, INTAKE, HELD);
    run(&mut app, 0.6);
    let halfway = lamps(&mut app, ZoneId::IntakeGallery);
    assert!(halfway < 0.7, "only part lit at 0.6 s, not {halfway}");
    run(&mut app, 0.7);
    for zone in [ZoneId::TowerHead, ZoneId::GateShaft, ZoneId::IntakeGallery] {
        assert_eq!(lamps(&mut app, zone), 1.0, "{zone:?} is lit after 1.3 s");
    }
    assert_eq!(lamps(&mut app, ZoneId::SpillwayGallery), 0.0, "another Section stays Dark");
}

#[test]
fn going_lit_flickers() {
    let mut app = playing(MapId::Dam);
    show(&mut app, INTAKE, HELD);
    let mut seen = Vec::new();
    for _ in 0..20 {
        app.update();
        seen.push(lamps(&mut app, ZoneId::IntakeGallery));
    }
    assert!(seen.windows(2).any(|pair| pair[1] < pair[0]), "it dips on the way up: {seen:?}");
}

#[test]
fn going_dark_drops_faster_than_going_lit() {
    let mut app = playing(MapId::Dam);
    show(&mut app, INTAKE, HELD);
    run(&mut app, 1.5);
    show(&mut app, INTAKE, UNCLAIMED);
    run(&mut app, 0.5);
    assert_eq!(lamps(&mut app, ZoneId::IntakeGallery), 0.0, "Dark within half a second");
}

#[test]
fn the_control_room_lights_once_every_generator_is_held() {
    let mut app = playing(MapId::Dam);
    show(&mut app, 0, HELD);
    show(&mut app, 1, HELD);
    run(&mut app, 1.5);
    assert_eq!(lamps(&mut app, ZoneId::ControlRoom), 0.0, "two of three isn't enough");
    show(&mut app, 2, HELD);
    run(&mut app, 1.5);
    assert_eq!(lamps(&mut app, ZoneId::ControlRoom), 1.0);
}

#[test]
fn a_dark_map_has_no_ambient_light_and_the_others_keep_theirs() {
    let app = playing(MapId::Dam);
    assert_eq!(app.world().resource::<GlobalAmbientLight>().brightness, 0.0);
    let app = playing(MapId::Greybox);
    assert_eq!(app.world().resource::<GlobalAmbientLight>().brightness, AMBIENT_BRIGHTNESS);
}

/// A Player camera spawned on `app`'s Map, as `spawn_player` gives one.
fn camera(app: &mut App) -> Entity {
    let camera = app
        .world_mut()
        .spawn((
            crate::players::player::PlayerCamera,
            DistanceFog {
                color: Color::srgb(0.02, 0.02, 0.04),
                falloff: FogFalloff::Linear { start: FOG_START, end: FOG_END },
                ..default()
            },
        ))
        .id();
    app.update();
    camera
}

#[test]
fn on_a_dark_map_the_camera_blooms_in_close_black_fog() {
    let mut app = playing(MapId::Dam);
    let camera = camera(&mut app);
    let camera = app.world().entity(camera);
    assert!(camera.contains::<Hdr>() && camera.contains::<Bloom>(), "HDR and bloom from the first frame");
    let fog = camera.get::<DistanceFog>().unwrap();
    assert_eq!(fog.color, Color::BLACK);
    assert!(
        matches!(fog.falloff, FogFalloff::Linear { start, end } if start == DARK_FOG_START && end == DARK_FOG_END),
        "{:?}",
        fog.falloff
    );
}

#[test]
fn on_other_maps_the_camera_is_as_it_was() {
    let mut app = playing(MapId::Greybox);
    let camera = camera(&mut app);
    let camera = app.world().entity(camera);
    assert!(!camera.contains::<Hdr>() && !camera.contains::<Bloom>());
    let fog = camera.get::<DistanceFog>().unwrap();
    assert!(matches!(fog.falloff, FogFalloff::Linear { start, end } if start == FOG_START && end == FOG_END));
}

/// How brightly the chalk outlines in `zone` glow. Panics if they differ.
fn chalk_glow(app: &mut App, zone: ZoneId) -> f32 {
    let mut query = app.world_mut().query::<(&Chalk, &MeshMaterial3d<StandardMaterial>)>();
    let materials = app.world().resource::<Assets<StandardMaterial>>();
    let glows: Vec<f32> = query
        .iter(app.world())
        .filter(|(chalk, _)| chalk.0 == zone)
        .map(|(_, material)| materials.get(&material.0).unwrap().emissive.red)
        .collect();
    assert!(!glows.is_empty(), "{zone:?} has no chalk");
    assert!(glows.windows(2).all(|pair| pair[0] == pair[1]), "{zone:?}'s chalk differs: {glows:?}");
    glows[0]
}

#[test]
fn chalk_glows_faintly_only_in_dark_zones() {
    let mut app = playing(MapId::Dam);
    assert_eq!(chalk_glow(&mut app, ZoneId::CrestRoad), 0.0, "lit chalk looks as it does today");
    assert_eq!(chalk_glow(&mut app, ZoneId::TowerHead), CHALK_GLOW);
    show(&mut app, INTAKE, HELD);
    app.update();
    assert_eq!(chalk_glow(&mut app, ZoneId::TowerHead), 0.0);
    let mut app = playing(MapId::Greybox);
    let zone = MapId::Greybox.layout().wall_weapons[0].1;
    assert_eq!(chalk_glow(&mut app, zone), 0.0);
}
