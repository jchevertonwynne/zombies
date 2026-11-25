//! The Archive's lamps and daylight (#535), on a built Map.

use super::lamp_flicker::{self, LampFlicker};
use super::tests::{playing, playing_with, run};
use super::*;
use crate::maps::map::archive::daylight::{MOTES, Mote};

const CELLAR: [ZoneId; 5] = [ZoneId::FloodedCellar, ZoneId::Undercroft, ZoneId::StrongRoom, ZoneId::PumpRoom, ZoneId::WellStair];

/// The Archive, built and Playing, with the Power off.
fn archive() -> App {
    playing_with(MapId::Archive, |app| {
        app.init_resource::<crate::maps::power::Power>();
    })
}

fn throw_the_power(app: &mut App) {
    app.world_mut().resource_mut::<crate::maps::power::Power>().switch_on();
}

/// Each lamp's zone, strength as a fraction of full, and whether it carries a `LampFlicker`.
fn lamps(app: &mut App) -> Vec<(ZoneId, f32, bool)> {
    let look = app.world().resource::<MapId>().layout().look.lamps;
    let mut query = app.world_mut().query::<(&Lamp, &SpotLight, Option<&LampFlicker>)>();
    query.iter(app.world()).map(|(lamp, light, flicker)| (lamp.zone, light.intensity / (SPOT_LAMP_INTENSITY * look), flicker.is_some())).collect()
}

#[test]
fn a_sunless_look_casts_no_directional_shadows() {
    for (map, shadows) in [(MapId::Archive, false), (MapId::Greybox, true), (MapId::Polder, true), (MapId::Sanatorium, true)] {
        let mut app = playing(map);
        let mut query = app.world_mut().query::<&DirectionalLight>();
        let suns: Vec<&DirectionalLight> = query.iter(app.world()).collect();
        assert_eq!(suns.len(), 1, "{map:?}");
        assert_eq!(suns[0].shadow_maps_enabled, shadows, "{map:?}");
    }
}

#[test]
fn archive_cellar_lamps_run_at_0_5_before_the_power() {
    let mut app = archive();
    for _ in 0..200 {
        app.update();
        for (zone, level, flicker) in lamps(&mut app) {
            if CELLAR.contains(&zone) {
                assert!(flicker, "{zone:?}'s lamp has a LampFlicker");
                assert!((level - 0.5).abs() < 1e-4, "{zone:?} at {level} before the Power");
            }
        }
    }
    // The Power steps them to full (but a lamp mid-burst).
    throw_the_power(&mut app);
    app.update();
    for (zone, level, _) in lamps(&mut app).into_iter().filter(|(zone, ..)| CELLAR.contains(zone)) {
        assert!((level - 1.0).abs() < 1e-4 || (level - lamp_flicker::DIP_LEVEL).abs() < 1e-4, "{zone:?} at {level} after the Power");
    }
}

#[test]
fn archive_idle_flicker_only_in_the_cellar() {
    let mut app = archive();
    throw_the_power(&mut app);
    let found = lamps(&mut app);
    assert_eq!(found.len(), 88);
    assert!(found.iter().all(|&(zone, _, flicker)| flicker == CELLAR.contains(&zone)), "only the cellar's lamps carry a LampFlicker");
    let cellar = found.iter().filter(|(zone, ..)| CELLAR.contains(zone)).count();
    assert_eq!(cellar, 28);
    // About one in eight of them bursts.
    let mut query = app.world_mut().query::<&LampFlicker>();
    let bursting = query.iter(app.world()).filter(|flicker| flicker.flickers_when_idle()).count();
    assert!(bursting >= 1 && bursting * 3 < cellar, "{bursting} of {cellar}");
    assert!(query.iter(app.world()).all(|flicker| !flicker.stutters_when_dormant()));
    // Over two minutes the others never move and a bursting one dips.
    let mut dipped = false;
    for _ in 0..2400 {
        app.update();
        for (zone, level, _) in lamps(&mut app) {
            if CELLAR.contains(&zone) {
                assert!((lamp_flicker::DIP_LEVEL - 1e-4..=1.0 + 1e-4).contains(&level), "{level}");
                dipped |= level < 0.5;
            } else {
                assert!((level - 1.0).abs() < 1e-4, "{zone:?} at {level}");
            }
        }
    }
    assert!(dipped);
}

#[test]
fn archive_has_four_unshadowed_daylight_spots_and_160_motes() {
    let mut app = archive();
    run(&mut app, 0.5);
    let mut spots = app.world_mut().query_filtered::<&SpotLight, Without<Lamp>>();
    let spots: Vec<&SpotLight> = spots.iter(app.world()).collect();
    assert_eq!(spots.len(), 4);
    assert!(spots.iter().all(|spot| !spot.shadow_maps_enabled));
    assert_eq!(spots.iter().filter(|spot| spot.intensity == 600_000.0).count(), 1);
    assert_eq!(spots.iter().filter(|spot| spot.intensity == 250_000.0).count(), 3);
    let motes = app.world_mut().query::<&Mote>().iter(app.world()).count();
    assert_eq!(motes, 4 * MOTES as usize);
}
