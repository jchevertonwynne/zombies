//! The Sanatorium's ambience and the sounds of its Props (#526), from #505 section 8.
//!
//! - `RoomTone` (indoors) and `FogWind` (out of doors) loop flat under everything and crossfade in
//!   about half a second as the eye's Zone changes, as the fog's colour does.
//! - `FluorescentHum`: one emitter in each indoor Zone, from one of its lamps; only the four
//!   nearest the eye play.
//! - `LampFlicker`: the click of a lamp starting a flicker burst (the Look's `LampBurst`), at most
//!   once every 1.5 s.
//! - `BoilerDrone` at the Boiler House's boilers and `HeartMonitor` at the electrotherapy machine,
//!   while the Power is on.
//! - `TrolleySqueak`: a phantom from a random corridor or ward point every 30 to 90 s.
//!
//! The Archive (#542) has the same kind of ambience, with its own files: `ArchiveTone` on the reading and stack levels and
//! `CellarBed` in the five cellar Zones, crossfading in the same half second as the eye changes level; `Drip` at the cellar's
//! drip points (the four nearest the eye play); `BoilerDrone` at the Pump Room's engines while the Power is on; and three local
//! phantoms: `PageRustle` every 20 to 60 s from a stack hall or the Reading Room, `ShelfCreak` every 30 to 90 s from a fixed stack
//! and, rarely, `BookFall` from one. Its Ranges' cues are in `range.rs`.
//!
//! Every machine runs these from its own eye and clock: the Zone, the Power and the layout are the
//! same on all of them, so a Co-op client hears the same cues from the same places. The phantom
//! squeaks and the lamp bursts' timing are local and cosmetic by design. The pure rules are here
//! and tested without an `App`.

use std::ops::RangeInclusive;

use bevy::audio::Volume;
use bevy::prelude::*;

use super::Sounds;
use super::cues::Cue;
use crate::maps::darkness::lamp_flicker::LampBurst;
use crate::maps::map::archive::flood::{DRIPS, DripLanded};
use crate::maps::map::{Layout, MapId, PropKind};
use crate::maps::power::Power;
use crate::maps::zone::{PlayerZone, ZoneId};
use crate::players::player::{LocalPlayer, PlayerCamera};
use crate::state::{GameState, InRun};

/// How long the room tone and the wind take to trade places as the eye goes through a Door.
pub const AMBIENCE_FADE_SECS: f32 = 0.5;
/// How many fluorescent hums play at once: the nearest to the eye.
pub const HUM_VOICES: usize = 4;
/// The least time between two lamp-flicker clicks.
pub const LAMP_FLICKER_GAP_SECS: f32 = 1.5;
/// The seconds between one phantom trolley squeak and the next.
pub const SQUEAK_EVERY_SECS: RangeInclusive<f32> = 30.0..=90.0;
/// The Zones a phantom trolley squeaks in, and how high above a lamp's floor it sounds.
const SQUEAK_ZONES: [ZoneId; 5] =
    [ZoneId::WestCorridor, ZoneId::EastCorridor, ZoneId::LongWard, ZoneId::IsolationWard, ZoneId::ChildrensWard];
const SQUEAK_HEIGHT: f32 = 0.6;
/// A lamp hangs this far over its floor.
const LAMP_HEIGHT: f32 = 3.85;
/// How many drips play at once: the nearest to the eye.
pub const DRIP_VOICES: usize = 4;
/// The seconds between one phantom page turn and the next, one shelf creak and the next, and one fallen book and the next.
pub const RUSTLE_EVERY: RangeInclusive<f32> = 20.0..=60.0;
pub const CREAK_EVERY: RangeInclusive<f32> = 30.0..=90.0;
pub const FALL_EVERY: RangeInclusive<f32> = 120.0..=300.0;
/// The Zones a phantom page turn sounds in: the four stack halls and the Reading Room.
const RUSTLE_ZONES: [ZoneId; 5] = [ZoneId::OldStacks, ZoneId::FolioStacks, ZoneId::PeriodicalStacks, ZoneId::NewStacks, ZoneId::ReadingRoom];
/// How high above a floor a phantom page turn sounds, and how high up a fixed stack its creak or fallen book does (a share of its height).
const RUSTLE_HEIGHT: f32 = 1.4;
const STACK_HEIGHT_SHARE: f32 = 0.5;
/// The five Zones below the stacks: the cellar, where the bed plays in place of the tone.
const CELLAR_ZONES: [ZoneId; 5] = [ZoneId::WellStair, ZoneId::FloodedCellar, ZoneId::Undercroft, ZoneId::StrongRoom, ZoneId::PumpRoom];

/// Whether `zone` is one of the Archive's five cellar Zones.
pub fn cellar(zone: ZoneId) -> bool {
    CELLAR_ZONES.contains(&zone)
}

/// The two flat loops `map` keeps under it (see `Ambient`), if it has any: the Sanatorium's room tone and wind, the Archive's tone and bed.
pub fn ambient_cues(map: MapId) -> Option<[Cue; 2]> {
    match map {
        MapId::Sanatorium => Some([Cue::RoomTone, Cue::FogWind]),
        MapId::Archive => Some([Cue::ArchiveTone, Cue::CellarBed]),
        _ => None,
    }
}

/// How loud the flat ambience `cue` wants to be with the eye in `zone` of `layout`, from 0 to 1: the Sanatorium's by indoors or out, the
/// Archive's tone above the cellar and its bed in it.
pub fn zone_target(cue: Cue, layout: &Layout, zone: ZoneId) -> f32 {
    match cue {
        Cue::ArchiveTone => (!cellar(zone)).into(),
        Cue::CellarBed => cellar(zone).into(),
        _ => ambience_target(cue, layout.indoor(zone)),
    }
}

/// Whether the drip that has landed at `at` plays for an eye at `eye`: only the `DRIP_VOICES` drip points nearest it do.
pub fn drip_plays(eye: Vec3, at: Vec3) -> bool {
    let points: Vec<Vec3> = DRIPS.iter().map(|drip| drip.at).collect();
    nearest(eye, &points, DRIP_VOICES).into_iter().any(|i| points[i] == at)
}

/// How loud the flat ambience `cue` wants to be, from 0 to 1, with the eye indoors or out: the room
/// tone only indoors, the wind only out of doors.
pub fn ambience_target(cue: Cue, indoors: bool) -> f32 {
    match cue {
        Cue::RoomTone if indoors => 1.0,
        Cue::FogWind if !indoors => 1.0,
        _ => 0.0,
    }
}

/// `level` moved towards `target` by `dt` seconds of a fade that takes `AMBIENCE_FADE_SECS` end to end.
pub fn fade_toward(level: f32, target: f32, dt: f32) -> f32 {
    let step = dt / AMBIENCE_FADE_SECS;
    if (target - level).abs() <= step { target } else { level + step * (target - level).signum() }
}

/// The indices of the `n` of `points` nearest `eye`, nearest first.
pub fn nearest(eye: Vec3, points: &[Vec3], n: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..points.len()).collect();
    order.sort_by(|&a, &b| points[a].distance_squared(eye).total_cmp(&points[b].distance_squared(eye)));
    order.truncate(n);
    order
}

/// Whether a lamp-flicker click may sound at `now` seconds, the last having sounded at `last`.
pub fn lamp_flicker_due(last: Option<f32>, now: f32) -> bool {
    last.is_none_or(|last| now - last >= LAMP_FLICKER_GAP_SECS)
}

/// Seconds until the next phantom squeak, for a `roll` in `[0, 1)`.
pub fn next_squeak_secs(roll: f32) -> f32 {
    super::mix::within(&SQUEAK_EVERY_SECS, roll)
}

/// Where the Sanatorium's looping sounds come from, found once from its layout.
#[derive(Debug, Default)]
pub struct Emitters {
    /// One per indoor Zone, from its first lamp.
    pub hums: Vec<(ZoneId, Vec3)>,
    /// The Boiler House's boilers.
    pub boilers: Vec<Vec3>,
    /// The Treatment Room's electrotherapy machine.
    pub monitors: Vec<Vec3>,
    /// Where a phantom trolley may squeak.
    pub squeaks: Vec<Vec3>,
    /// Where a phantom page turn may sound: over the stack halls' and the Reading Room's floors.
    pub rustles: Vec<Vec3>,
    /// Where a phantom shelf creak or fallen book may sound: the middles of the fixed stacks.
    pub stacks: Vec<Vec3>,
}

/// The emitters of `map`: on the Sanatorium a hum in each indoor Zone, the boilers and the electrotherapy machine (the Machine Props of
/// the Boiler House and the Treatment Room), and the squeak points; on the Archive its pump engines, and the points a page turn, a
/// creak or a fallen book may sound from. Every other Map has none.
pub fn emitters(map: MapId) -> Emitters {
    let layout = map.layout();
    match map {
        MapId::Sanatorium => {
            let machines = |zone: ZoneId| -> Vec<Vec3> {
                layout.props.iter().filter(|prop| prop.zone == zone && prop.kind == PropKind::Machine).map(|prop| prop.at).collect()
            };
            Emitters {
                hums: layout
                    .zones
                    .iter()
                    .copied()
                    .filter(|&zone| layout.indoor(zone))
                    .filter_map(|zone| layout.lamps.iter().find(|(lamp_zone, _)| *lamp_zone == zone).map(|&(_, at)| (zone, at)))
                    .collect(),
                boilers: machines(ZoneId::BoilerHouse),
                monitors: machines(ZoneId::TreatmentRoom),
                squeaks: layout
                    .lamps
                    .iter()
                    .filter(|(zone, _)| SQUEAK_ZONES.contains(zone))
                    .map(|&(_, at)| Vec3::new(at.x, at.y - LAMP_HEIGHT + SQUEAK_HEIGHT, at.z))
                    .collect(),
                ..default()
            }
        }
        MapId::Archive => Emitters {
            boilers: layout.pump_engines().collect(),
            rustles: layout
                .lamps
                .iter()
                .filter(|(zone, _)| RUSTLE_ZONES.contains(zone))
                .map(|&(zone, at)| Vec3::new(at.x, zone.floor() + RUSTLE_HEIGHT, at.z))
                .collect(),
            stacks: layout
                .props
                .iter()
                .filter(|prop| prop.kind == PropKind::Stack)
                .map(|prop| prop.at + Vec3::Y * prop.size.y * STACK_HEIGHT_SHARE)
                .collect(),
            ..default()
        },
        _ => Emitters::default(),
    }
}

/// The looping sounds wanted at points right now, each as (cue, key, where): the nearest hums to
/// `eye`, and the boilers' drone and the monitor's beep while the Power is on. A key tells one
/// emitter of a cue from another.
pub fn wanted_loops(emitters: &Emitters, eye: Option<Vec3>, power_on: bool) -> Vec<(Cue, usize, Vec3)> {
    let mut wanted = Vec::new();
    if let Some(eye) = eye {
        let points: Vec<Vec3> = emitters.hums.iter().map(|&(_, at)| at).collect();
        wanted.extend(nearest(eye, &points, HUM_VOICES).into_iter().map(|i| (Cue::FluorescentHum, i, points[i])));
    }
    if power_on {
        wanted.extend(emitters.boilers.iter().enumerate().map(|(i, &at)| (Cue::BoilerDrone, i, at)));
        wanted.extend(emitters.monitors.iter().enumerate().map(|(i, &at)| (Cue::HeartMonitor, i, at)));
    }
    wanted
}

/// One of the two flat loops under the Sanatorium, and how loud it is now, from 0 to 1.
#[derive(Component)]
pub(super) struct Ambient {
    cue: Cue,
    level: f32,
}

/// A looping sound at a point in the world.
#[derive(Component)]
pub(super) struct PointLoop {
    cue: Cue,
    key: usize,
}

/// The Map this Run is being played on, if it has ambience (see `ambient_cues`) and it is being played.
fn ambient_map(map: &Option<Res<MapId>>, state: &State<GameState>) -> Option<MapId> {
    let map = map.as_deref().copied()?;
    (*state.get() == GameState::Playing && ambient_cues(map).is_some()).then_some(map)
}

/// Keeps the room tone and the wind looping, and fades each to what the eye's Zone wants.
pub(super) fn ambience(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    map: Option<Res<MapId>>,
    state: Res<State<GameState>>,
    time: Res<Time>,
    zones: Query<&PlayerZone, With<LocalPlayer>>,
    mut playing: Query<(Entity, &mut Ambient, Option<&mut AudioSink>)>,
) {
    let Some(map) = ambient_map(&map, &state) else {
        for (entity, ..) in &playing {
            commands.entity(entity).despawn();
        }
        return;
    };
    let layout = map.layout();
    // Before the eye has a Zone it is where the Run begins.
    let zone = zones.iter().next().map_or_else(|| layout.start_zone(), |zone| zone.0);
    for cue in ambient_cues(map).into_iter().flatten() {
        if !playing.iter().any(|(_, ambient, _)| ambient.cue == cue)
            && let Some(sound) = sounds.looping(cue, 0.0)
        {
            commands.spawn((InRun, Ambient { cue, level: 0.0 }, sound));
        }
    }
    for (_, mut ambient, sink) in &mut playing {
        let level = fade_toward(ambient.level, zone_target(ambient.cue, layout, zone), time.delta_secs());
        ambient.level = level;
        if let Some(mut sink) = sink {
            sink.set_volume(Volume::Linear(sounds.gain(ambient.cue) * level));
        }
    }
}

/// Keeps the nearest hums, and the boilers' drone and the heart monitor while the Power is on,
/// looping from where they are.
pub(super) fn point_loops(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    map: Option<Res<MapId>>,
    state: Res<State<GameState>>,
    power: Res<Power>,
    eyes: Query<&GlobalTransform, With<PlayerCamera>>,
    playing: Query<(Entity, &PointLoop)>,
    mut found: Local<Option<(MapId, Emitters)>>,
) {
    let Some(map) = ambient_map(&map, &state) else {
        for (entity, _) in &playing {
            commands.entity(entity).despawn();
        }
        return;
    };
    let emitters = found_emitters(&mut found, map);
    let wanted = wanted_loops(emitters, eyes.iter().next().map(GlobalTransform::translation), power.is_on());
    for (entity, sound) in &playing {
        if !wanted.iter().any(|&(cue, key, _)| cue == sound.cue && key == sound.key) {
            commands.entity(entity).despawn();
        }
    }
    for (cue, key, at) in wanted {
        if playing.iter().any(|(_, sound)| sound.cue == cue && sound.key == key) {
            continue;
        }
        if let Some(sound) = sounds.looping(cue, 1.0) {
            commands.spawn((InRun, PointLoop { cue, key }, sound)).insert(Transform::from_translation(at));
        }
    }
}

/// The click of a lamp starting a flicker burst, from where the lamp hangs, at most once every
/// `LAMP_FLICKER_GAP_SECS`.
pub(super) fn lamp_flicker_clicks(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    mut bursts: MessageReader<LampBurst>,
    time: Res<Time>,
    mut last: Local<Option<f32>>,
) {
    let now = time.elapsed_secs();
    for burst in bursts.read() {
        if lamp_flicker_due(*last, now) {
            *last = Some(now);
            sounds.play(&mut commands, Cue::LampFlicker, Some(burst.at));
        }
    }
}

/// The emitters of `map`, found once and kept in `found`.
fn found_emitters(found: &mut Option<(MapId, Emitters)>, map: MapId) -> &Emitters {
    if found.as_ref().is_none_or(|(kept, _)| *kept != map) {
        *found = Some((map, emitters(map)));
    }
    &found.as_ref().expect("just set").1
}

/// The phantom sounds `emitters` has places for, each as (cue, the seconds between one and the next, where it may sound from).
pub fn phantoms(emitters: &Emitters) -> [(Cue, RangeInclusive<f32>, &[Vec3]); 4] {
    [
        (Cue::TrolleySqueak, SQUEAK_EVERY_SECS, &emitters.squeaks),
        (Cue::PageRustle, RUSTLE_EVERY, &emitters.rustles),
        (Cue::ShelfCreak, CREAK_EVERY, &emitters.stacks),
        (Cue::BookFall, FALL_EVERY, &emitters.stacks),
    ]
}

/// Now and then a trolley squeaks from somewhere in a corridor or a ward, or a page turns, a shelf creaks or a book falls somewhere in the
/// Archive, though no one is there. Each machine rolls its own: it is for atmosphere, not a fact of the game.
pub(super) fn phantom_sounds(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    map: Option<Res<MapId>>,
    state: Res<State<GameState>>,
    time: Res<Time>,
    mut found: Local<Option<(MapId, Emitters)>>,
    mut next: Local<[Option<f32>; 4]>,
) {
    let Some(map) = ambient_map(&map, &state) else {
        *next = [None; 4];
        return;
    };
    let now = time.elapsed_secs();
    let emitters = found_emitters(&mut found, map);
    for (slot, (cue, every, points)) in phantoms(emitters).into_iter().enumerate() {
        if points.is_empty() {
            continue;
        }
        let due = *next[slot].get_or_insert_with(|| now + super::mix::within(&every, rand::random()));
        if now < due {
            continue;
        }
        next[slot] = Some(now + super::mix::within(&every, rand::random()));
        let pick = ((rand::random::<f32>() * points.len() as f32) as usize).min(points.len() - 1);
        sounds.play(&mut commands, cue, Some(points[pick]));
    }
}

/// A drip falling at one of the cellar's drip points sounds there, if it is one of the four nearest the eye. Each machine's drips fall on
/// its own clock (`maps::map::archive::flood`), so this is a cosmetic, like the ripple it goes with.
pub(super) fn drips(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    mut landed: MessageReader<DripLanded>,
    eyes: Query<&GlobalTransform, With<PlayerCamera>>,
) {
    let eye = eyes.iter().next().map(GlobalTransform::translation);
    for drip in landed.read() {
        if eye.is_some_and(|eye| drip_plays(eye, drip.at)) {
            sounds.play(&mut commands, Cue::Drip, Some(drip.at));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sanatorium() -> Emitters {
        emitters(MapId::Sanatorium)
    }

    #[test]
    fn room_tone_plays_only_indoors() {
        assert_eq!(ambience_target(Cue::RoomTone, true), 1.0);
        assert_eq!(ambience_target(Cue::RoomTone, false), 0.0);
        // And the wind is its mirror, so the two trade places.
        assert_eq!(ambience_target(Cue::FogWind, false), 1.0);
        assert_eq!(ambience_target(Cue::FogWind, true), 0.0);
        // By Zone: the Hall is indoors, the Forecourt and the Sun Roof are not.
        let layout = MapId::Sanatorium.layout();
        assert!(layout.indoor(ZoneId::EntranceHall));
        assert!(!layout.indoor(ZoneId::Forecourt));
        assert!(!layout.indoor(ZoneId::SunRoof));
    }

    #[test]
    fn the_ambience_crossfades_in_half_a_second() {
        let mut level = 0.0;
        let mut secs = 0.0;
        while level < 1.0 {
            level = fade_toward(level, 1.0, 1.0 / 60.0);
            secs += 1.0 / 60.0;
        }
        assert!((secs - AMBIENCE_FADE_SECS).abs() < 0.05, "{secs}");
        assert_eq!(fade_toward(1.0, 1.0, 0.1), 1.0);
        assert!(fade_toward(1.0, 0.0, 0.1) < 1.0);
        assert_eq!(fade_toward(0.05, 0.0, 1.0), 0.0, "and it stops at the target");
    }

    #[test]
    fn there_is_one_hum_emitter_in_each_of_the_fourteen_indoor_zones() {
        let layout = MapId::Sanatorium.layout();
        let hums = sanatorium().hums;
        assert_eq!(hums.len(), 14);
        let mut zones: Vec<_> = hums.iter().map(|(zone, _)| *zone).collect();
        zones.sort_by_key(|zone| format!("{zone:?}"));
        zones.dedup();
        assert_eq!(zones.len(), 14, "one each");
        assert!(zones.iter().all(|&zone| layout.indoor(zone)));
    }

    #[test]
    fn only_the_four_nearest_hums_play() {
        let emitters = sanatorium();
        // Standing in each indoor Zone in turn.
        for &(_, eye) in &emitters.hums {
            let wanted = wanted_loops(&emitters, Some(eye), false);
            let hums: Vec<_> = wanted.iter().filter(|(cue, ..)| *cue == Cue::FluorescentHum).collect();
            assert_eq!(hums.len(), HUM_VOICES);
            // The eye's own Zone's hum is the nearest, and nothing beyond the fourth is heard.
            let reach = hums.iter().map(|(_, _, at)| at.distance(eye)).fold(0.0, f32::max);
            let heard = emitters.hums.iter().filter(|(_, at)| at.distance(eye) < reach).count();
            assert!(heard < HUM_VOICES, "{heard} are nearer than the fourth");
        }
        // Nowhere to hear from, nothing plays.
        assert!(wanted_loops(&emitters, None, false).is_empty());
    }

    #[test]
    fn the_nearest_are_returned_nearest_first() {
        let points = [Vec3::new(9.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(5.0, 0.0, 0.0), Vec3::new(3.0, 0.0, 0.0)];
        assert_eq!(nearest(Vec3::ZERO, &points, 3), vec![1, 3, 2]);
        assert_eq!(nearest(Vec3::ZERO, &points, 9).len(), 4);
    }

    #[test]
    fn lamp_flicker_sound_at_most_once_every_1_5_s() {
        assert!(lamp_flicker_due(None, 0.0));
        assert!(!lamp_flicker_due(Some(10.0), 10.0));
        assert!(!lamp_flicker_due(Some(10.0), 11.4));
        assert!(lamp_flicker_due(Some(10.0), 11.5));
        // Driven as the system does: bursts every 0.4 s for 10 s click at most 7 times.
        let mut last = None;
        let mut clicks = 0;
        for step in 0..25 {
            let now = step as f32 * 0.4;
            if lamp_flicker_due(last, now) {
                last = Some(now);
                clicks += 1;
            }
        }
        assert!((6..=7).contains(&clicks), "{clicks}");
    }

    #[test]
    fn boiler_drone_and_heart_monitor_play_only_with_the_power() {
        let emitters = sanatorium();
        let eye = Some(Vec3::ZERO);
        let without = wanted_loops(&emitters, eye, false);
        assert!(without.iter().all(|(cue, ..)| !matches!(cue, Cue::BoilerDrone | Cue::HeartMonitor)), "{without:?}");
        let with = wanted_loops(&emitters, eye, true);
        let drones: Vec<_> = with.iter().filter(|(cue, ..)| *cue == Cue::BoilerDrone).collect();
        let beeps: Vec<_> = with.iter().filter(|(cue, ..)| *cue == Cue::HeartMonitor).collect();
        assert_eq!(drones.len(), 2, "the two boilers");
        assert_eq!(beeps.len(), 1, "the electrotherapy machine");
        // They sound from the Props, in their Zones.
        assert!(drones.iter().all(|(_, _, at)| at.x < -50.0), "{drones:?}");
        assert!(beeps.iter().all(|(_, _, at)| (at.x - 47.2).abs() < 0.1 && (at.z - 43.4).abs() < 0.1), "{beeps:?}");
        // The hums don't wait for the Power.
        assert!(without.iter().any(|(cue, ..)| *cue == Cue::FluorescentHum));
    }

    #[test]
    fn phantom_squeaks_come_from_corridors_and_wards_every_30_to_90_s() {
        let squeaks = sanatorium().squeaks;
        assert!(squeaks.len() >= 8, "{}", squeaks.len());
        assert!(squeaks.iter().all(|at| at.y > 0.0 && at.y < 8.0));
        assert_eq!(next_squeak_secs(0.0), 30.0);
        assert_eq!(next_squeak_secs(1.0), 90.0);
        assert!((30.0..=90.0).contains(&next_squeak_secs(0.37)));
    }

    #[test]
    fn the_sanatorium_sounds_total_under_two_megabytes() {
        const SLOTS: [&str; 14] = [
            "quarantine_klaxon", "quarantine_siren", "quarantine_swing", "quarantine_slam", "quarantine_release", "crash_bar",
            "door_pound", "room_tone", "fog_wind", "fluorescent_hum", "lamp_flicker", "boiler_drone", "heart_monitor",
            "trolley_squeak",
        ];
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join(super::super::cues::AUDIO_DIR);
        let files = super::super::slots::list_files(&dir);
        let mut total = 0;
        for slot in SLOTS {
            let variants = super::super::slots::variants(slot, &files).unwrap_or_else(|error| panic!("{error}"));
            total += variants.iter().map(|file| std::fs::metadata(dir.join(file)).unwrap().len()).sum::<u64>();
        }
        assert!(total < 2_000_000, "the Sanatorium's sounds are {total} bytes");
    }

    fn archive() -> Emitters {
        emitters(MapId::Archive)
    }

    /// The tone is under the reading and stack levels and the bed under the five cellar Zones, one or the other everywhere: the two are
    /// each other's mirror, so the crossfade is always one rising as the other falls.
    #[test]
    fn archive_tone_and_cellar_bed_split_by_level() {
        let layout = MapId::Archive.layout();
        let mut cellar_zones = Vec::new();
        for &zone in layout.zones {
            let (tone, bed) = (zone_target(Cue::ArchiveTone, layout, zone), zone_target(Cue::CellarBed, layout, zone));
            assert_eq!(tone + bed, 1.0, "{zone:?}: one plays, not both, not neither");
            if bed == 1.0 {
                cellar_zones.push(zone);
            }
        }
        cellar_zones.sort_by_key(|zone| format!("{zone:?}"));
        let mut want = vec![ZoneId::WellStair, ZoneId::FloodedCellar, ZoneId::Undercroft, ZoneId::StrongRoom, ZoneId::PumpRoom];
        want.sort_by_key(|zone| format!("{zone:?}"));
        assert_eq!(cellar_zones, want, "the five cellar Zones");
        for zone in [
            ZoneId::CatalogueHall,
            ZoneId::ReadingRoom,
            ZoneId::MapRoom,
            ZoneId::LectureTheatre,
            ZoneId::OldStacks,
            ZoneId::FolioStacks,
            ZoneId::PeriodicalStacks,
            ZoneId::NewStacks,
            ZoneId::Bindery,
        ] {
            assert_eq!(zone_target(Cue::ArchiveTone, layout, zone), 1.0, "{zone:?} is a reading or stack Zone");
        }
        // Each Map keeps its own pair, and nothing else keeps any.
        assert_eq!(ambient_cues(MapId::Archive), Some([Cue::ArchiveTone, Cue::CellarBed]));
        assert_eq!(ambient_cues(MapId::Sanatorium), Some([Cue::RoomTone, Cue::FogWind]));
        assert_eq!(ambient_cues(MapId::Cove), None);
        // The crossfade between them takes the same half second.
        let mut level = 0.0;
        let mut secs = 0.0;
        while level < 1.0 {
            level = fade_toward(level, zone_target(Cue::CellarBed, layout, ZoneId::FloodedCellar), 1.0 / 60.0);
            secs += 1.0 / 60.0;
        }
        assert!((secs - AMBIENCE_FADE_SECS).abs() < 0.05, "{secs}");
        // The Sanatorium's pair still goes by indoors and out.
        let sanatorium = MapId::Sanatorium.layout();
        assert_eq!(zone_target(Cue::RoomTone, sanatorium, ZoneId::EntranceHall), 1.0);
        assert_eq!(zone_target(Cue::FogWind, sanatorium, ZoneId::Forecourt), 1.0);
    }

    #[test]
    fn only_the_four_nearest_drips_play() {
        let points: Vec<Vec3> = DRIPS.iter().map(|drip| drip.at).collect();
        assert_eq!(points.len(), 6);
        // From each drip point, from the middle of the cellar and from far off: exactly four of the six play, the four nearest.
        let mut eyes: Vec<Vec3> = points.iter().map(|&at| at + Vec3::Y * 1.6).collect();
        eyes.extend([Vec3::new(0.0, 2.0, 10.0), Vec3::new(-30.0, 8.0, 2.0), Vec3::new(30.0, 8.0, 30.0), Vec3::new(-12.0, 1.6, 28.0)]);
        for eye in eyes {
            let playing: Vec<usize> = (0..points.len()).filter(|&i| drip_plays(eye, points[i])).collect();
            assert_eq!(playing.len(), DRIP_VOICES, "eye {eye}: {playing:?}");
            let reach = playing.iter().map(|&i| points[i].distance(eye)).fold(0.0, f32::max);
            assert!((0..points.len()).filter(|i| !playing.contains(i)).all(|i| points[i].distance(eye) >= reach), "eye {eye}: a nearer drip is silent");
        }
        // Standing at the foot of the Well Stair its own drip is heard, and the Pool's far one is not.
        let stair = Vec3::new(-22.5, 1.6, 1.0);
        assert!(drip_plays(stair, DRIPS[5].at) && !drip_plays(stair, DRIPS[2].at));
    }

    #[test]
    fn boiler_drone_plays_only_with_the_power_on_the_archive() {
        let emitters = archive();
        let engines: Vec<Vec3> = MapId::Archive.layout().pump_engines().collect();
        assert_eq!(engines.len(), 2, "the Pump Room's two engines");
        assert_eq!(emitters.boilers, engines);
        let eye = Some(Vec3::new(0.0, 2.0, 10.0));
        let without = wanted_loops(&emitters, eye, false);
        assert!(without.is_empty(), "no hum, no monitor, no drone before the Power: {without:?}");
        let with = wanted_loops(&emitters, eye, true);
        assert_eq!(with.len(), 2);
        assert!(with.iter().all(|(cue, _, at)| *cue == Cue::BoilerDrone && engines.contains(at)), "{with:?}");
        // They are the Pump Room's.
        let (low, high) = ZoneId::PumpRoom.volume();
        assert!(engines.iter().all(|at| low.x <= at.x && at.x <= high.x && low.z <= at.z && at.z <= high.z), "{engines:?}");
    }

    #[test]
    fn the_archives_phantoms_have_places_and_their_own_timings() {
        let emitters = archive();
        assert!(emitters.rustles.len() >= 20, "{}", emitters.rustles.len());
        assert_eq!(emitters.stacks.len(), 8, "two fixed stacks in each of the four stack halls");
        assert!(emitters.hums.is_empty() && emitters.squeaks.is_empty() && emitters.monitors.is_empty(), "nothing of the Sanatorium's");
        let kinds = phantoms(&emitters);
        let secs: Vec<_> = kinds.iter().map(|(cue, every, points)| (*cue, *every.start(), *every.end(), !points.is_empty())).collect();
        assert_eq!(secs[1], (Cue::PageRustle, 20.0, 60.0, true));
        assert_eq!(secs[2], (Cue::ShelfCreak, 30.0, 90.0, true));
        assert_eq!(secs[3].0, Cue::BookFall);
        assert!(secs[3].1 > secs[2].2, "a fallen book is rarer than a creak");
        assert!(!secs[0].3, "no trolley in the Archive");
        let sanatorium = sanatorium();
        assert!(phantoms(&sanatorium).iter().all(|(cue, _, points)| *cue == Cue::TrolleySqueak || points.is_empty()));
    }

    /// Footsteps come from `Material::family`, so the Archive's new Material Sets step on the families that have files: the wooden floors
    /// on wood, the stone floors on tile, the brick and slate of the cellar on brick or tile, and the concrete of the Strong Room.
    #[test]
    fn the_archives_floors_step_on_wood_stone_and_tile_with_files() {
        use super::super::cues::FloorFamily;
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join(super::super::cues::AUDIO_DIR);
        let files = super::super::slots::list_files(&dir);
        let mut heard = std::collections::HashSet::new();
        for &zone in MapId::Archive.layout().zones {
            let (low, high) = zone.volume();
            let feet = Vec3::new((low.x + high.x) / 2.0, zone.floor(), (low.z + high.z) / 2.0);
            let family = crate::maps::map::floor_family(MapId::Archive, feet).unwrap_or_else(|| panic!("{zone:?} has no floor at {feet}"));
            let slot = super::super::slots::variants(family.slot(), &files).unwrap_or_else(|error| panic!("{zone:?}: {error}"));
            assert!(!slot.is_empty(), "{zone:?} steps on {family:?}, which has no files");
            heard.insert(family);
        }
        for family in [FloorFamily::Wood, FloorFamily::Tile, FloorFamily::Brick, FloorFamily::Concrete] {
            assert!(heard.contains(&family), "no Archive Zone steps on {family:?}");
        }
        // The Material Sets the look decision names: wood for the parquet and boards, stone and slate for tile.
        use crate::maps::map::textures::Material;
        assert_eq!(Material::HerringboneParquet.family(), FloorFamily::Wood);
        assert_eq!(Material::OldWoodenFloor02.family(), FloorFamily::Wood);
        assert_eq!(Material::MonasteryStoneFloor.family(), FloorFamily::Tile);
        assert_eq!(Material::SlateFloor02.family(), FloorFamily::Tile);
    }

    #[test]
    fn the_archives_sounds_total_under_two_megabytes() {
        const SLOTS: [(&str, usize); 10] = [
            ("range_rattle", 1),
            ("range_roll", 1),
            ("range_thud", 2),
            ("wheel_turn", 2),
            ("archive_tone", 1),
            ("cellar_bed", 1),
            ("drip", 3),
            ("page_rustle", 4),
            ("shelf_creak", 3),
            ("book_fall", 2),
        ];
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join(super::super::cues::AUDIO_DIR);
        let files = super::super::slots::list_files(&dir);
        let mut total = 0;
        for (slot, want) in SLOTS {
            let variants = super::super::slots::variants(slot, &files).unwrap_or_else(|error| panic!("{error}"));
            assert_eq!(variants.len(), want, "{slot}");
            total += variants.iter().map(|file| std::fs::metadata(dir.join(file)).unwrap().len()).sum::<u64>();
        }
        assert!(total < 2_000_000, "the Archive's sounds are {total} bytes");
    }
}
