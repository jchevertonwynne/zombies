//! The Sanatorium's fluorescent tubes (#522), on a built Map.


use super::lamp_flicker::{self, LampFlicker};
use super::tests::{FRAME, playing, playing_with, run};
use super::*;

/// The Sanatorium, built and Playing, with the Power off.
fn sanatorium() -> App {
    playing_with(MapId::Sanatorium, |app| {
        app.init_resource::<crate::maps::power::Power>();
    })
}

fn throw_the_power(app: &mut App) {
    app.world_mut().resource_mut::<crate::maps::power::Power>().switch_on();
}

/// A Tube lamp's strength as a fraction of full, whether it stutters (Dormant), whether it bursts
/// (idle), and its Zone and position.
struct Tube {
    level: f32,
    stutters: bool,
    bursts: bool,
    zone: ZoneId,
    at: Vec3,
}

/// Whether `level` is `want`, give or take rounding.
fn is(level: f32, want: f32) -> bool {
    (level - want).abs() < 1e-4
}

fn tubes(app: &mut App) -> Vec<Tube> {
    let look = app.world().resource::<MapId>().layout().look.lamps;
    let mut query = app.world_mut().query::<(&Lamp, &SpotLight, &LampFlicker, &Transform)>();
    query
        .iter(app.world())
        .map(|(lamp, light, tube, at)| Tube {
            level: light.intensity / (SPOT_LAMP_INTENSITY * look),
            stutters: tube.stutters_when_dormant(),
            bursts: tube.flickers_when_idle(),
            zone: lamp.zone,
            at: at.translation,
        })
        .collect()
}

#[test]
fn only_the_sanatoriums_indoor_lamps_but_the_boiler_houses_are_tubes() {
    let mut app = sanatorium();
    let found = tubes(&mut app);
    let layout = MapId::Sanatorium.layout();
    let expected = layout.lamps.iter().filter(|(zone, _)| layout.indoor(*zone) && *zone != ZoneId::BoilerHouse).count();
    assert_eq!(found.len(), expected);
    // No other Map's lamps have one.
    let mut other = playing(MapId::Greybox);
    assert!(tubes(&mut other).is_empty());
}

#[test]
fn dormant_lamps_run_at_0_6() {
    let mut app = sanatorium();
    let mut steady = 0;
    for _ in 0..400 {
        app.update();
        for tube in tubes(&mut app) {
            if tube.stutters {
                assert!(is(tube.level, 0.6) || is(tube.level, lamp_flicker::DIP_LEVEL), "a stutterer is at 0.6 or dipped, not {}", tube.level);
            } else {
                assert!((tube.level - 0.6).abs() < 1e-5, "a steady lamp is at 0.6, not {}", tube.level);
                steady += 1;
            }
        }
    }
    assert!(steady > 0);
    // About one in three stutters.
    let all = tubes(&mut app);
    let stutterers = all.iter().filter(|tube| tube.stutters).count();
    assert!(stutterers * 6 > all.len() && stutterers * 2 < all.len(), "{stutterers} of {}", all.len());
}

#[test]
fn the_power_steps_the_lamps_to_full() {
    let mut app = sanatorium();
    run(&mut app, 1.0);
    throw_the_power(&mut app);
    app.update();
    for tube in tubes(&mut app) {
        assert!(is(tube.level, 1.0) || (tube.bursts && is(tube.level, lamp_flicker::DIP_LEVEL)), "{:?} at {}", tube.zone, tube.level);
    }
}

#[test]
fn lamp_flicker_stays_within_0_1_and_1() {
    let mut app = sanatorium();
    for powered in [false, true] {
        if powered {
            throw_the_power(&mut app);
        }
        let mut dipped = false;
        for _ in 0..1200 {
            app.update();
            for tube in tubes(&mut app) {
                assert!((0.1..=1.0).contains(&tube.level), "{}", tube.level);
                dipped |= tube.level < 0.5;
            }
        }
        assert!(dipped, "some lamp dips within a minute (powered: {powered})");
    }
}

#[test]
fn no_idle_flicker_in_the_operating_theatre() {
    let mut app = sanatorium();
    throw_the_power(&mut app);
    let mut seen = 0;
    for _ in 0..1200 {
        app.update();
        for tube in tubes(&mut app).into_iter().filter(|tube| tube.zone == ZoneId::OperatingTheatre) {
            assert!(!tube.bursts);
            assert!(is(tube.level, 1.0), "the Theatre's lamps are steady");
            seen += 1;
        }
    }
    assert!(seen > 0);
}

#[test]
fn idle_flicker_picks_the_same_lamps_every_run() {
    let picked = |app: &mut App| -> Vec<Vec3> { tubes(app).into_iter().filter(|tube| tube.bursts).map(|tube| tube.at).collect() };
    let (mut first, mut second) = (sanatorium(), sanatorium());
    let (a, b) = (picked(&mut first), picked(&mut second));
    assert_eq!(a, b);
    let all = tubes(&mut first).len();
    assert!(a.len() * 12 > all && a.len() * 3 < all, "about one in six: {} of {all}", a.len());
    // And the choice is a pure function of the position.
    for &at in &a {
        assert_eq!(lamp_flicker::position_hash(at) % 6, 0);
    }
}

#[test]
fn a_burst_is_dips_of_40_to_120_ms() {
    let mut app = sanatorium();
    throw_the_power(&mut app);
    let target = tubes(&mut app).into_iter().find(|tube| tube.bursts).expect("one lamp bursts").at;
    let (mut dips, mut longest, mut shortest, mut run_len, mut was_dipped) = (0, 0.0_f32, f32::MAX, 0.0_f32, false);
    for _ in 0..4000 {
        app.update();
        let dipped = tubes(&mut app).into_iter().find(|tube| tube.at == target).unwrap().level < 0.5;
        match (was_dipped, dipped) {
            (false, true) => {
                dips += 1;
                run_len = FRAME;
            }
            (true, true) => run_len += FRAME,
            (true, false) => {
                longest = longest.max(run_len);
                shortest = shortest.min(run_len);
            }
            _ => {}
        }
        was_dipped = dipped;
    }
    assert!(dips >= 2, "the lamp dipped {dips} times in 200 s");
    assert!(shortest >= 0.04 - 1e-3 && longest <= 0.12 * 5.0 + 0.12 * 4.0 + FRAME, "(a frame can span a gap, so a burst's dips may merge) dips last {shortest}..{longest} s");
}

// ---- The Quarantine's stages on a Wing's lamps (#524) ----

use crate::maps::quarantine::{Quarantine, Stage, WingId};

/// The Sanatorium Playing with the Power on and a Quarantine to show stages on.
fn quarantined() -> App {
    let mut app = playing_with(MapId::Sanatorium, |app| {
        app.init_resource::<crate::maps::power::Power>().init_resource::<Quarantine>();
    });
    throw_the_power(&mut app);
    run(&mut app, 1.0);
    app
}

fn show(app: &mut App, stage: Stage, wing: u8, elapsed: f32) {
    app.world_mut().resource_mut::<Quarantine>().show(stage, Some(WingId(wing)), elapsed);
}

/// The levels of the tube lamps in `zones`, over `frames` frames.
fn levels_of(app: &mut App, zones: &[ZoneId], frames: u32) -> Vec<Vec<f32>> {
    (0..frames)
        .map(|_| {
            app.update();
            tubes(app).into_iter().filter(|tube| zones.contains(&tube.zone)).map(|tube| tube.level).collect()
        })
        .collect()
}

const WEST: [ZoneId; 2] = [ZoneId::LongWard, ZoneId::DayRoom];

#[test]
fn wing_lamps_flicker_hard_through_warning_and_sealing() {
    for stage in [Stage::Warning, Stage::Sealing] {
        let mut app = quarantined();
        show(&mut app, stage, 0, 1.0);
        let mut seen: Vec<f32> = Vec::new();
        let mut changes = 0;
        let mut last = Vec::new();
        for frame in levels_of(&mut app, &WEST, 200) {
            assert!(frame.iter().all(|&level| (lamp_flicker::HARD_FLICKER_MIN - 1e-4..=1.0 + 1e-4).contains(&level)), "within [0.1, 1]: {frame:?}");
            if frame != last {
                changes += 1;
            }
            last = frame.clone();
            seen.extend(frame);
        }
        // 200 frames of 0.05 s is 10 s: about 6 changes a second, over the Wing's lamps.
        assert!(changes > 40, "{stage:?}: it flickers hard, {changes} changes in 10 s");
        assert!(seen.iter().any(|&level| is(level, 0.1)), "{stage:?}: it dips to 0.1");
        assert!(seen.iter().any(|&level| level > 0.9), "{stage:?}: and comes back up");
        // The lamps of the other Wings and the corridor outside do not.
        let calm = levels_of(&mut app, &[ZoneId::BathHall, ZoneId::WestCorridor], 40);
        assert!(calm.iter().flatten().all(|&level| level >= lamp_flicker::DIP_LEVEL - 1e-4));
        assert!(calm.iter().flatten().filter(|&&level| is(level, 1.0)).count() > calm.iter().flatten().count() / 2, "{stage:?}");
    }
}

#[test]
fn sealed_wing_lamps_hold_at_0_35() {
    let mut app = quarantined();
    show(&mut app, Stage::Sealed, 0, 1.0);
    for frame in levels_of(&mut app, &WEST, 400) {
        assert!(!frame.is_empty() && frame.iter().all(|&level| is(level, 0.35)), "{frame:?}");
    }
    // At Release they fade back to full over 1.2 s with a stutter, never below 0.1.
    show(&mut app, Stage::Release, 0, 0.0);
    let mut peak = 0.0_f32;
    for _ in 0..30 {
        app.world_mut().resource_mut::<Quarantine>().run_shown(FRAME);
        for level in levels_of(&mut app, &WEST, 1).remove(0) {
            assert!((lamp_flicker::HARD_FLICKER_MIN - 1e-4..=1.0 + 1e-4).contains(&level), "{level}");
            peak = peak.max(level);
        }
    }
    assert!(peak > 0.9, "back at full by the end of the fade, {peak}");
}
