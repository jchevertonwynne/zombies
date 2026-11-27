//! The Refinery's lamps, floodlights and probes (#568), on a built Map.

use super::flood::{self, FloodLight, FloodOn};
use super::lamp_flicker::{self, LampFlicker};
use super::tests::{FRAME, playing_with, run};
use super::*;
use crate::maps::map::LampStyle;
use crate::maps::power::Power;

const CONTROL_HOUSE: ZoneId = ZoneId::ControlHouse;

/// The Refinery, built and Playing, with the Power off.
fn refinery() -> App {
    playing_with(MapId::Refinery, |app| {
        app.init_resource::<Power>();
    })
}

fn throw_the_power(app: &mut App) {
    app.world_mut().resource_mut::<Power>().switch_on();
}

/// Each flood's index and its fraction of full.
fn floods(app: &mut App) -> Vec<(usize, f32)> {
    let mut query = app.world_mut().query::<(&FloodLight, &SpotLight)>();
    let mut out: Vec<_> = query.iter(app.world()).map(|(flood, light)| (flood.index, light.intensity / flood::FLOOD_LUMENS)).collect();
    out.sort_by_key(|&(index, _)| index);
    out
}

/// The strength of every lamp in `zone`, as a fraction of full.
fn lamp_levels(app: &mut App, zone: ZoneId) -> Vec<f32> {
    let mut query = app.world_mut().query::<(&Lamp, &SpotLight)>();
    query.iter(app.world()).filter(|(lamp, _)| lamp.zone == zone).map(|(_, light)| light.intensity / SPOT_LAMP_INTENSITY).collect()
}

#[test]
fn refinery_light_count_is_at_most_110() {
    let layout = MapId::Refinery.layout();
    let blazes: usize = layout.fuel_lines.iter().map(|line| line.blazes.len()).sum();
    // The layout's lamps, the floods, one light a Blaze and the flare.
    let total = layout.lamps.len() + layout.floods.len() + blazes + 1;
    assert_eq!((layout.lamps.len(), layout.floods.len(), blazes), (87, 6, 7));
    assert_eq!(total, 101);
    assert!(total <= 110);
}

#[test]
fn the_built_refinery_has_its_lamps_floods_and_flare() {
    let mut app = refinery();
    let mut points = app.world_mut().query::<&PointLight>();
    let mut spots = app.world_mut().query::<&SpotLight>();
    // The flare and a light for each of the seven Blazes are point lights; every lamp and flood is a spot.
    assert_eq!(points.iter(app.world()).count(), 1 + 7);
    assert_eq!(spots.iter(app.world()).count(), 87 + 6);
}

/// The Blazes' lights bring the spawned count from 94 (87 lamps, 6 floods, the flare) to the 101 the look decision budgets, which the
/// light budget (110) holds: every light a built Refinery has, counted on the built Map.
#[test]
fn the_built_refinerys_light_count_is_101_with_the_blazes_and_within_the_budget() {
    let mut app = refinery();
    let lights = app.world_mut().query::<&PointLight>().iter(app.world()).count() + app.world_mut().query::<&SpotLight>().iter(app.world()).count();
    assert_eq!(lights, 101);
    assert!(lights <= 110);
    // Each Blaze's light is shadowless and flickering, never one of the lamps that `shadow_nearest_lamps` might shadow.
    let mut blazes = app.world_mut().query::<(&PointLight, &crate::maps::fire_look::BlazeSite, Has<Lamp>)>();
    let all: Vec<_> = blazes.iter(app.world()).collect();
    assert_eq!(all.len(), 7);
    assert!(all.iter().all(|(light, _, is_lamp)| !light.shadow_maps_enabled && !is_lamp));
}

#[test]
fn floods_are_off_before_the_power_and_on_after() {
    let mut app = refinery();
    run(&mut app, 2.0);
    let before = floods(&mut app);
    assert_eq!(before.len(), 6);
    assert!(before.iter().all(|&(_, level)| level == 0.0), "{before:?}");
    throw_the_power(&mut app);
    run(&mut app, flood::beat_secs(6) + 0.2);
    let after = floods(&mut app);
    assert!(after.iter().all(|&(_, level)| (level - 1.0).abs() < 1e-4), "{after:?}");
}

#[test]
fn floods_come_on_0_3_s_apart_with_a_warm_up_stutter() {
    let mut app = refinery();
    run(&mut app, 1.0);
    let mut on: Vec<(usize, f32)> = Vec::new();
    let mut history: Vec<Vec<f32>> = vec![Vec::new(); 6];
    throw_the_power(&mut app);
    for frame in 1..=((flood::beat_secs(6) + 0.5) / FRAME) as u32 {
        app.update();
        let now = frame as f32 * FRAME;
        for (index, level) in floods(&mut app) {
            history[index].push(level);
            if level > 0.0 && !on.iter().any(|&(i, _)| i == index) {
                on.push((index, now));
            }
        }
    }
    assert_eq!(on.len(), 6);
    // Each comes on 0.3 s after the one before it (to a frame).
    for pair in on.windows(2) {
        assert!(((pair[1].1 - pair[0].1) - flood::FLOOD_GAP_SECS).abs() <= FRAME + 1e-3, "{on:?}");
    }
    // Each stutters while it warms up: it dips back to `DIP` after it is first on, comes up again, and ends full.
    for (index, levels) in history.iter().enumerate() {
        let first = levels.iter().position(|&level| level > 0.0).unwrap();
        let lit = &levels[first..];
        assert!(lit.iter().any(|&level| (level - flood::DIP).abs() < 1e-4), "flood {index} never dipped: {lit:?}");
        assert!(lit[1..].iter().any(|&level| level > 0.4), "flood {index} never came up");
        assert!((levels.last().unwrap() - 1.0).abs() < 1e-4);
    }
}

#[test]
fn the_stutter_is_the_same_everywhere() {
    // A pure function of the flood and the time since the Power, so every machine agrees.
    let layout = MapId::Refinery.layout();
    for (index, flood) in layout.floods.iter().enumerate() {
        let curve: Vec<f32> = (0..60).map(|step| flood::level(index, flood.at, step as f32 * 0.05)).collect();
        assert_eq!(curve, (0..60).map(|step| flood::level(index, flood.at, step as f32 * 0.05)).collect::<Vec<_>>());
        assert_eq!(flood::level(index, flood.at, index as f32 * flood::FLOOD_GAP_SECS - 0.01), 0.0);
        assert_eq!(flood::level(index, flood.at, 100.0), 1.0);
    }
}

#[test]
fn each_flood_sends_flood_on_as_it_comes_on() {
    let mut app = refinery();
    run(&mut app, 0.5);
    throw_the_power(&mut app);
    let mut seen = Vec::new();
    for _ in 0..((flood::beat_secs(6) + 0.5) / FRAME) as u32 {
        app.update();
        seen.extend(app.world_mut().resource_mut::<Messages<FloodOn>>().drain().map(|on| on.zone));
    }
    let layout = MapId::Refinery.layout();
    assert_eq!(seen, layout.floods.iter().map(|flood| flood.zone).collect::<Vec<_>>());
}

#[test]
fn a_machine_already_powered_sees_the_floods_steady() {
    // A late joiner: the Power is already on when its first frame runs, so there's no beat to play.
    let mut app = playing_with(MapId::Refinery, |app| {
        app.init_resource::<Power>();
        app.world_mut().resource_mut::<Power>().switch_on();
    });
    app.update();
    assert!(floods(&mut app).iter().all(|&(_, level)| (level - 1.0).abs() < 1e-4));
    assert!(app.world_mut().resource_mut::<Messages<FloodOn>>().drain().next().is_none());
}

#[test]
fn shadow_nearest_lamps_skips_floods() {
    let mut app = refinery();
    // The eye stands right under a flood, which also carries a `Lamp` here, to prove the filter and not a missing component is what skips it.
    let at = MapId::Refinery.layout().floods[0].at;
    let flood = app.world_mut().query_filtered::<Entity, With<FloodLight>>().iter(app.world()).next().unwrap();
    app.world_mut().entity_mut(flood).insert(Lamp::in_zone(ZoneId::Gatehouse));
    app.world_mut().spawn((PlayerCamera, GlobalTransform::from_translation(at - Vec3::Y)));
    app.add_systems(Update, crate::maps::look::shadow_nearest_lamps);
    run(&mut app, 0.2);
    let mut spots = app.world_mut().query::<(&SpotLight, Has<FloodLight>)>();
    let all: Vec<_> = spots.iter(app.world()).collect();
    assert!(all.iter().filter(|(_, is_flood)| *is_flood).all(|(light, _)| !light.shadow_maps_enabled));
    assert_eq!(all.iter().filter(|(light, _)| light.shadow_maps_enabled).count(), SHADOWED_LAMPS);
}

#[test]
fn no_idle_flicker_on_the_refinery() {
    let mut app = refinery();
    throw_the_power(&mut app);
    run(&mut app, 3.0);
    let mut query = app.world_mut().query::<&LampFlicker>();
    for flicker in query.iter(app.world()) {
        assert!(!flicker.flickers_when_idle());
        assert!(!flicker.stutters_when_dormant());
    }
    // Over two minutes every lamp holds steady at full once the Power is on, and no burst is ever sent.
    let mut lamps = app.world_mut().query::<(&Lamp, &SpotLight)>();
    for _ in 0..2400 {
        app.update();
        for (lamp, light) in lamps.iter(app.world()) {
            assert!((light.intensity / SPOT_LAMP_INTENSITY - 1.0).abs() < 1e-4, "{:?} at {}", lamp.zone, light.intensity / SPOT_LAMP_INTENSITY);
        }
        assert!(app.world_mut().resource_mut::<Messages<lamp_flicker::LampBurst>>().drain().next().is_none());
    }
}

#[test]
fn control_house_lamps_run_at_0_5_before_the_power() {
    let mut app = refinery();
    for _ in 0..200 {
        app.update();
        let house = lamp_levels(&mut app, CONTROL_HOUSE);
        assert_eq!(house.len(), 12);
        assert!(house.iter().all(|level| (level - 0.5).abs() < 1e-4), "{house:?}");
        // The plant never stopped, so every yard lamp is full from the start.
        let yard = lamp_levels(&mut app, ZoneId::Gatehouse);
        assert!(yard.iter().all(|level| (level - 1.0).abs() < 1e-4), "{yard:?}");
    }
    throw_the_power(&mut app);
    app.update();
    assert!(lamp_levels(&mut app, CONTROL_HOUSE).iter().all(|level| (level - 1.0).abs() < 1e-4));
}

#[test]
fn the_refinerys_lamps_wear_their_styles() {
    let layout = MapId::Refinery.layout();
    let count = |style: LampStyle| layout.lamps.iter().filter(|&&(zone, _)| layout.lamp_style_in(zone) == style).count();
    // 28 sodium posts; 17 catwalk bulbs and 30 caged shed bulbs; the Control House's 12 tubes.
    assert_eq!((count(LampStyle::Sodium), count(LampStyle::Bulb), count(LampStyle::Tube)), (28, 17 + 30, 12));
    assert_eq!(layout.lamp_style_in(ZoneId::RackWalk), LampStyle::Bulb);
    assert_eq!(layout.lamp_style_in(ZoneId::Gatehouse), LampStyle::Sodium);
    assert_eq!(layout.lamp_style_in(ZoneId::ControlHouse), LampStyle::Tube);
}

#[test]
fn refinery_has_three_indoor_probes() {
    let probes = crate::maps::map::probes(MapId::Refinery);
    let layout = MapId::Refinery.layout();
    assert_eq!(probes.len(), 3);
    let zones: Vec<_> = probes.iter().map(|probe| probe.zone.unwrap()).collect();
    assert_eq!(zones, [ZoneId::FittingShop, ZoneId::CompressorHouse, ZoneId::ControlHouse]);
    for probe in &probes {
        assert_eq!(probe.brightness, INDOOR_PROBE_BRIGHTNESS);
        let lamp = crate::maps::map::palette(probe.zone).light;
        assert_eq!(probe.colour, layout.look.probe_colour(lamp));
        // 40% of the way from the constant towards the lamp: neither the constant nor the lamp.
        assert_ne!(probe.colour, crate::maps::look::INDOOR_PROBE_COLOUR);
        assert_ne!(probe.colour, lamp);
    }
    // The Control House's is the cold one.
    let cold = LinearRgba::from(probes[2].colour);
    assert!(cold.blue > cold.red);
}

/// Each of the six floods stands on a mast (#574): one `FloodMast` a flood, on the floor under it, as high as the flood is over its Zone's floor, with its
/// stand-in post built (the lattice and the heads replace it only once their files are on disk and a world loads them, never in the headless runs). No
/// other Map has floods, so none has masts.
#[test]
fn every_flood_has_a_mast() {
    let mut app = refinery();
    app.update();
    let layout = MapId::Refinery.layout();
    let mut query = app.world_mut().query::<(&flood::FloodMast, &Transform, &Children)>();
    let mut masts: Vec<_> = query.iter(app.world()).map(|(mast, at, children)| (mast.index, mast.top, at.translation, children.len())).collect();
    masts.sort_by_key(|&(index, ..)| index);
    assert_eq!(masts.len(), layout.floods.len());
    for (index, top, base, kids) in masts {
        let flood = &layout.floods[index];
        assert_eq!((base.x, base.z), (flood.at.x, flood.at.z), "mast {index} stands under its flood");
        assert_eq!(base.y, flood.zone.floor(), "mast {index} stands on the floor");
        assert!((top - (flood.at.y - flood.zone.floor())).abs() < 1e-4, "mast {index} reaches its flood");
        assert_eq!(kids, 1, "mast {index} has its stand-in post");
    }
    // The models' files are on disk, for a game that loads them.
    assert!(crate::maps::map::textures::on_disk(crate::maps::map::FLOOD_MAST.0) && crate::maps::map::textures::on_disk(crate::maps::map::FLOOD_HEADS.0));
    let mut other = playing_with(MapId::Greybox, |_| {});
    other.update();
    assert_eq!(other.world_mut().query::<&flood::FloodMast>().iter(other.world()).count(), 0);
}

#[test]
fn other_maps_have_no_floods() {
    for map in MapId::ALL {
        assert_eq!(map.layout().floods.len(), if map == MapId::Refinery { 6 } else { 0 }, "{map:?}");
    }
    // Every flood stands in its own Zone, high on a mast over it, with the spec's metal-halide lamp.
    for flood in MapId::Refinery.layout().floods {
        let (lo, hi) = flood.zone.volume();
        assert!(flood.at.x >= lo.x && flood.at.x <= hi.x && flood.at.z >= lo.z && flood.at.z <= hi.z, "{flood:?} outside {:?}", flood.zone);
        assert!(flood.at.y - flood.zone.floor() >= 8.0);
    }
    assert_eq!((flood::FLOOD_LUMENS, flood::FLOOD_RANGE, flood::FLOOD_CONE), (4_000_000.0, 45.0, 0.45));
}
