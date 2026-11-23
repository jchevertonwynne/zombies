use std::time::Duration;

use bevy::ecs::system::RunSystemOnce;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;

use super::weapon_light::WeaponLight;
use super::*;
use crate::maps::generator::Generators;
use crate::maps::map::MapId;
use crate::players::player::{Seat, player_body};
use crate::state::GameState;

/// A frame of this many seconds.
pub(super) const FRAME: f32 = 0.05;

/// `map` built and Playing, with its Generators all Unclaimed and Darkness running.
pub(super) fn playing(map: MapId) -> App {
    playing_with(map, |_| {})
}

/// The same, with `extra` added first.
pub(super) fn playing_with(map: MapId, extra: impl FnOnce(&mut App)) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), TransformPlugin, StatesPlugin));
    extra(&mut app);
    app
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .insert_state(GameState::Playing)
        .insert_resource(map)
        .insert_resource(Generators::for_map(map))
        .insert_resource(GlobalAmbientLight::default())
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(FRAME)))
        .add_plugins(DarknessPlugin);
    app.finish();
    app.cleanup();
    app.world_mut().run_system_once(crate::maps::map::spawn_map).unwrap();
    app.update();
    app
}

/// Runs `secs` of frames.
pub(super) fn run(app: &mut App, secs: f32) {
    for _ in 0..(secs / FRAME).round() as u32 {
        app.update();
    }
}

/// How bright `zone`'s lamps are, from 0 off to 1 full (as strong as the Map's Look has them). Panics if they differ.
fn lamps(app: &mut App, zone: ZoneId) -> f32 {
    let layout = app.world().resource::<MapId>().layout();
    let mut query = app.world_mut().query::<(&Lamp, Option<&PointLight>, Option<&SpotLight>)>();
    let levels: Vec<f32> = query
        .iter(app.world())
        .filter(|(lamp, ..)| lamp.zone == zone)
        .map(|(_, point, spot)| match (point, spot) {
            (Some(light), _) => light.intensity / (LAMP_INTENSITY * layout.look.lamps),
            (_, Some(light)) => light.intensity / (SPOT_LAMP_INTENSITY * layout.look.lamps),
            _ => panic!("a lamp with no light"),
        })
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

/// A Player camera spawned on `app`'s Map, as `spawn_player` gives one.
fn camera(app: &mut App) -> Entity {
    let camera = app
        .world_mut()
        .spawn((
            crate::players::player::PlayerCamera,
            Transform::default(),
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
    assert_eq!(chalk_glow(&mut app, ZoneId::CrestRoad), CHALK_FAINT_GLOW, "lit chalk glows only faintly");
    assert_eq!(chalk_glow(&mut app, ZoneId::TowerHead), CHALK_GLOW);
    show(&mut app, INTAKE, HELD);
    app.update();
    assert_eq!(chalk_glow(&mut app, ZoneId::TowerHead), CHALK_FAINT_GLOW);
    let mut app = playing(MapId::Greybox);
    let zone = MapId::Greybox.layout().wall_weapons[0].1;
    assert_eq!(chalk_glow(&mut app, zone), CHALK_FAINT_GLOW);
}

/// The Weapon Lights in `app`, each with the entity it hangs from.
fn weapon_lights(app: &mut App) -> Vec<(Entity, SpotLight)> {
    let mut query = app.world_mut().query_filtered::<(&ChildOf, &SpotLight), With<WeaponLight>>();
    query.iter(app.world()).map(|(parent, light)| (parent.parent(), *light)).collect()
}

/// A teammate on `app`'s Map, in Seat 1, as a Co-op Run spawns one.
fn teammate(app: &mut App, map: MapId) -> Entity {
    let teammate = app.world_mut().spawn(player_body(Seat(1), "Player 2", &[], map)).id();
    app.update();
    teammate
}

#[test]
fn on_a_dark_map_the_camera_carries_a_narrow_shadow_casting_weapon_light() {
    let mut app = playing(MapId::Dam);
    let camera = camera(&mut app);
    let lights = weapon_lights(&mut app);
    assert_eq!(lights.len(), 1, "one light, on the camera");
    let (on, light) = &lights[0];
    assert_eq!(*on, camera);
    assert!((light.range - 30.0).abs() < 1.0, "about 30 m: {}", light.range);
    assert!((light.inner_angle - 0.2).abs() < 0.01 && (light.outer_angle - 0.42).abs() < 0.01);
    assert!(light.intensity > 0.0, "always on");
    assert!(light.shadow_maps_enabled, "it casts shadows");
}

#[test]
fn on_other_maps_nobody_carries_a_weapon_light() {
    let mut app = playing(MapId::Greybox);
    camera(&mut app);
    teammate(&mut app, MapId::Greybox);
    run(&mut app, 0.1);
    assert!(weapon_lights(&mut app).is_empty());
}

/// Where `app`'s one Weapon Light hanging from `parent` is, and which way it points, in the world.
fn light_on(app: &mut App, parent: Entity) -> (Vec3, Vec3) {
    let mut query = app.world_mut().query_filtered::<(&ChildOf, &GlobalTransform), With<WeaponLight>>();
    let lights: Vec<_> = query.iter(app.world()).filter(|(on, _)| on.parent() == parent).map(|(_, at)| *at).collect();
    assert_eq!(lights.len(), 1, "one Weapon Light on {parent:?}");
    (lights[0].translation(), *lights[0].forward())
}

#[test]
fn each_teammate_carries_one_at_eye_height_pointing_where_they_look() {
    let mut app = playing(MapId::Dam);
    let body = teammate(&mut app, MapId::Dam);
    let pitch = -0.4;
    let yaw = 1.0;
    {
        let mut entity = app.world_mut().entity_mut(body);
        entity.get_mut::<crate::players::player::MoveInput>().unwrap().pitch = pitch;
        entity.get_mut::<Transform>().unwrap().rotation = Quat::from_rotation_y(yaw);
    }
    run(&mut app, 0.1);
    let (_, light) = weapon_lights(&mut app).into_iter().find(|(on, _)| *on == body).expect("the teammate has a light");
    assert!(light.shadow_maps_enabled, "a teammate's light casts shadows too");
    let centre = app.world().entity(body).get::<GlobalTransform>().unwrap().translation();
    let (at, pointing) = light_on(&mut app, body);
    let look = crate::weapons::muzzle::aim_direction(yaw, pitch);
    assert!(pointing.dot(look) > 0.999, "points where they look: {pointing} vs {look}");
    let eye = centre + Vec3::Y * PLAYER_EYE_HEIGHT;
    let from_eye = at - eye;
    assert!(from_eye.normalize().dot(look) > 0.999, "out along the look from their eye: {from_eye}");
    assert!(from_eye.length() > 1.0, "ahead of their Weapon: {from_eye}");
}

#[test]
fn the_camera_light_sits_ahead_of_the_longest_view_model_muzzle() {
    for row in crate::tuning::WEAPON_HANDLING {
        let muzzle = crate::weapons::weapon::view_muzzle_depth(row.class);
        assert!(WEAPON_LIGHT_AHEAD > muzzle + 0.1, "{:?}'s muzzle is {muzzle} m ahead", row.class);
    }
    let mut app = playing(MapId::Dam);
    let camera = camera(&mut app);
    let (at, pointing) = light_on(&mut app, camera);
    assert_eq!(pointing, Vec3::NEG_Z);
    assert!((at - Vec3::NEG_Z * WEAPON_LIGHT_AHEAD).length() < 1e-4, "dead ahead, past the muzzle: {at}");
}

#[test]
fn a_weapon_light_comes_in_short_of_a_wall_in_reach() {
    assert_eq!(weapon_light::reach(None), WEAPON_LIGHT_AHEAD);
    assert_eq!(weapon_light::reach(Some(10.0)), WEAPON_LIGHT_AHEAD);
    assert!((weapon_light::reach(Some(0.6)) - 0.5).abs() < 1e-6);
    assert_eq!(weapon_light::reach(Some(0.0)), 0.0, "never behind the eye");
}

/// A view model on `map`'s camera, its part, and a mesh elsewhere in the world: whether each
/// casts a shadow.
fn view_model_casts(map: MapId) -> (bool, bool, bool) {
    let mut app = playing(map);
    let camera = camera(&mut app);
    let gun = app.world_mut().spawn((Mesh3d::default(), ChildOf(camera))).id();
    let part = app.world_mut().spawn((Mesh3d::default(), ChildOf(gun))).id();
    let elsewhere = app.world_mut().spawn(Mesh3d::default()).id();
    app.update();
    let casts = |e: Entity| !app.world().entity(e).contains::<bevy::light::NotShadowCaster>();
    (casts(gun), casts(part), casts(elsewhere))
}

#[test]
fn on_a_dark_map_nothing_on_the_camera_casts_a_shadow() {
    assert_eq!(view_model_casts(MapId::Dam), (false, false, true), "the view model doesn't; the world still does");
}

#[test]
fn on_other_maps_the_view_model_casts_shadows_as_it_did() {
    for map in [MapId::Greybox, MapId::Quarry, MapId::Village] {
        assert_eq!(view_model_casts(map), (true, true, true), "on the {}", map.name());
    }
}

#[test]
fn a_weapon_light_never_reaches_through_a_wall() {
    use avian3d::prelude::*;
    let mut app = playing_with(MapId::Dam, |app| {
        app.add_plugins(PhysicsPlugins::default());
    });
    // Off the Map, a teammate faces a wall 0.7 m from their eye.
    let body = teammate(&mut app, MapId::Dam);
    let centre = Vec3::new(0.0, 500.0, 0.0);
    let eye = centre + Vec3::Y * PLAYER_EYE_HEIGHT;
    app.world_mut().entity_mut(body).insert((Transform::from_translation(centre), Position(centre)));
    app.world_mut().spawn((RigidBody::Static, Collider::cuboid(4.0, 4.0, 0.2), Transform::from_translation(eye - Vec3::Z * 0.8)));
    run(&mut app, 0.2);
    let (at, _) = light_on(&mut app, body);
    assert!((at - (eye - Vec3::Z * (0.7 - WEAPON_LIGHT_WALL_GAP))).length() < 0.02, "just short of the wall: {at}");
}
