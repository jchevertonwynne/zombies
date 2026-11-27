//! The Refinery's Fuel Lines: what each one sounds like through its cycle (#575, from #510 section 9).
//!
//! - `BlazeLoop` crackles from each Lit Blaze, and only the `BLAZE_VOICES` nearest the eye play.
//! - `GasHiss` loops from each Blaze and Valve of a line through its Flare-up.
//! - `PipeClank` sounds as a line Shuts, `BlazeIgnite` from each of its Blazes as it lights again, and `ValveTurn` at the Valve a Player
//!   stood beside as it was used (a Shut, or a relight).
//! - `BridgeClunk` at each floodlight coming on (`FloodOn`).
//!
//! Every machine works all of it out from the `FuelLines` it shows, which the Host's `WorldEvent::FuelLine` sets (and the catch-up gives a
//! late joiner), as `range::range_cues` does for a Range, so the Host, each client and a late joiner (who hears nothing for what they
//! missed) hear the same line from where it is, and nothing is sent on the wire. The floods follow the replicated `Power`, so each machine
//! sees its own `FloodOn`. A Valve is only heard turning when a Player is standing at it: a Boss Round's Shut and a Sabotaged line's
//! relight have no hand on a wheel.

use std::collections::HashMap;

use bevy::prelude::*;

use super::ambience::nearest;
use super::cues::Cue;
use super::{PlayCue, Sounds};
use crate::maps::darkness::flood::FloodOn;
use crate::maps::fire::{FuelLine, FuelLineId, FuelLines, LinePhase};
use crate::maps::map::{Layout, MapId};
use crate::players::player::{Player, PlayerCamera};
use crate::state::{GameState, InRun};
use crate::tuning::USE_RANGE;

/// How many Lit Blazes crackle at once: the nearest to the eye.
pub const BLAZE_VOICES: usize = 3;
/// How many hisses sound at once: the nearest to the eye (a line has at most three Blazes and two Valves, so this is only reached when
/// several lines flare up together).
pub const HISS_VOICES: usize = 8;
/// How near a Player stands to a Valve (in x and z) for the wheel they turn to be that one: a use is made from beside it.
pub const VALVE_EARSHOT: f32 = USE_RANGE + 2.0;
/// How high above a floor a Blaze's crackle, ignition and hiss sound, and a Valve's hiss and turn (its wheel).
const BLAZE_HEIGHT: f32 = 0.5;
const VALVE_HEIGHT: f32 = 1.1;
/// A Valve's hiss keys start here, so they are never a Blaze's.
const VALVE_KEY: usize = 1000;

/// Where a Blaze's sounds come from.
fn blaze_point(blaze: &crate::maps::fire::Blaze) -> Vec3 {
    blaze.centre() + Vec3::Y * BLAZE_HEIGHT
}

/// Where the Valve at `at` is heard from: at its wheel.
fn valve_point(at: Vec3) -> Vec3 {
    at + Vec3::Y * VALVE_HEIGHT
}

/// The Valve of `line` a Player at one of `players` stands beside, as it is used: the one nearest any of them, if one is within
/// `VALVE_EARSHOT`. `None` when no one is at a Valve of this line (a Boss Round's Shut, a Sabotaged line's relight).
pub fn valve_used(line: &FuelLine, players: &[Vec3]) -> Option<Vec3> {
    line.valves
        .iter()
        .filter_map(|&(_, at, _)| {
            let nearest = players
                .iter()
                .filter(|player| (player.y - at.y).abs() < 3.0)
                .map(|player| Vec2::new(player.x - at.x, player.z - at.z).length())
                .fold(f32::INFINITY, f32::min);
            (nearest <= VALVE_EARSHOT).then_some((valve_point(at), nearest))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(at, _)| at)
}

/// The cues a line makes as it goes from `before` to `now`, each with where it sounds from: the `PipeClank` of a Shut (from its first
/// Blaze, where the feed pipe runs) and the `ValveTurn` of the Valve a Player used, for a Shut or a relight; the `BlazeIgnite` at each
/// Blaze as it lights again.
pub fn line_cues(line: &FuelLine, before: LinePhase, now: LinePhase, players: &[Vec3]) -> Vec<(Cue, Vec3)> {
    let mut cues = Vec::new();
    match (before, now) {
        (LinePhase::Lit, LinePhase::Shut { .. }) => {
            if let Some(blaze) = line.blazes.first() {
                cues.push((Cue::PipeClank, blaze_point(blaze)));
            }
            cues.extend(valve_used(line, players).map(|at| (Cue::ValveTurn, at)));
        }
        (LinePhase::Shut { .. }, LinePhase::FlareUp { .. }) => cues.extend(valve_used(line, players).map(|at| (Cue::ValveTurn, at))),
        (LinePhase::FlareUp { .. }, LinePhase::Lit) => cues.extend(line.blazes.iter().map(|blaze| (Cue::BlazeIgnite, blaze_point(blaze)))),
        _ => {}
    }
    cues
}

/// Asks for each Fuel Line's one-shot cues as its phase changes, on every machine from the phase it shows.
pub fn fuel_line_cues(
    lines: Res<FuelLines>,
    players: Query<&Transform, With<Player>>,
    map: Option<Res<MapId>>,
    state: Option<Res<State<GameState>>>,
    mut was: Local<HashMap<FuelLineId, LinePhase>>,
    mut cues: MessageWriter<PlayCue>,
) {
    // Between Runs the lines are reset to Lit: that is not a line lighting.
    let Some(map) = map.filter(|_| state.is_none_or(|state| *state.get() == GameState::Playing)) else {
        was.clear();
        return;
    };
    let layout = map.layout();
    let players: Vec<Vec3> = players.iter().map(|transform| transform.translation).collect();
    for (i, line) in layout.fuel_lines.iter().enumerate() {
        let id = FuelLineId(i as u8);
        let now = lines.phase(id);
        let before = was.insert(id, now);
        // Only a change the machine has watched makes a sound: a phase first seen (a late joiner's) does not.
        let Some(before) = before else { continue };
        if std::mem::discriminant(&before) == std::mem::discriminant(&now) {
            continue;
        }
        for (cue, at) in line_cues(line, before, now, &players) {
            cues.write(PlayCue::new(cue, Some(at)));
        }
    }
}

/// The looping sounds the Fuel Lines want right now, each as (cue, key, where): the `BLAZE_VOICES` Lit Blazes nearest `eye` crackle, and
/// the `HISS_VOICES` Blazes and Valves nearest it of the lines in their Flare-up hiss. A key tells one emitter of a cue from another.
pub fn wanted_loops(layout: &Layout, lines: &FuelLines, eye: Option<Vec3>) -> Vec<(Cue, usize, Vec3)> {
    let Some(eye) = eye else { return Vec::new() };
    let (mut lit, mut hissing) = (Vec::new(), Vec::new());
    let (mut blaze_key, mut valve_key) = (0, VALVE_KEY);
    for (i, line) in layout.fuel_lines.iter().enumerate() {
        let phase = lines.phase(FuelLineId(i as u8));
        for blaze in line.blazes {
            match phase {
                LinePhase::Lit => lit.push((blaze_key, blaze_point(blaze))),
                LinePhase::FlareUp { .. } => hissing.push((blaze_key, blaze_point(blaze))),
                LinePhase::Shut { .. } => {}
            }
            blaze_key += 1;
        }
        for &(_, at, _) in line.valves {
            if matches!(phase, LinePhase::FlareUp { .. }) {
                hissing.push((valve_key, valve_point(at)));
            }
            valve_key += 1;
        }
    }
    let mut wanted = Vec::new();
    for (cue, points, voices) in [(Cue::BlazeLoop, lit, BLAZE_VOICES), (Cue::GasHiss, hissing, HISS_VOICES)] {
        let spots: Vec<Vec3> = points.iter().map(|&(_, at)| at).collect();
        wanted.extend(nearest(eye, &spots, voices).into_iter().map(|i| (cue, points[i].0, points[i].1)));
    }
    wanted
}

/// A Fuel Line's crackle or hiss, looping at a point.
#[derive(Component)]
pub(super) struct FireLoop {
    cue: Cue,
    key: usize,
}

/// Keeps the nearest Lit Blazes crackling and the Flare-up's hiss going, each from where it is, and nothing else.
pub(super) fn fire_loops(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    map: Option<Res<MapId>>,
    state: Res<State<GameState>>,
    lines: Res<FuelLines>,
    eyes: Query<&GlobalTransform, With<PlayerCamera>>,
    playing: Query<(Entity, &FireLoop)>,
) {
    let wanted = match (&map, *state.get() == GameState::Playing) {
        (Some(map), true) => wanted_loops(map.layout(), &lines, eyes.iter().next().map(GlobalTransform::translation)),
        _ => Vec::new(),
    };
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
            commands.spawn((InRun, FireLoop { cue, key }, sound)).insert(Transform::from_translation(at));
        }
    }
}

/// A flood coming on clunks, from the flood, once. The floods follow the replicated Power, so each machine hears its own.
pub fn flood_clunks(mut on: MessageReader<FloodOn>, mut cues: MessageWriter<PlayCue>) {
    for on in on.read() {
        cues.write(PlayCue::new(Cue::BridgeClunk, Some(on.at)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maps::fire::Blaze;
    use crate::tuning::{FLARE_UP_SECS, SHUT_SECS};

    fn refinery() -> &'static Layout {
        MapId::Refinery.layout()
    }

    fn lines() -> impl Iterator<Item = &'static FuelLine> {
        refinery().fuel_lines.iter()
    }

    fn cue_names(cues: &[(Cue, Vec3)]) -> Vec<Cue> {
        cues.iter().map(|&(cue, _)| cue).collect()
    }

    const SHUT: LinePhase = LinePhase::Shut { elapsed: 0.0 };
    const FLARE: LinePhase = LinePhase::FlareUp { elapsed: 0.0 };

    /// A clank as a line Shuts, a hiss through its Flare-up, an ignite at each Blaze as it lights and a valve turn at the wheel a Player is
    /// at, for a Shut and for a relight; nothing at any other change, and nothing for the seconds running on.
    #[test]
    fn fuel_line_cues_follow_the_phase() {
        assert_eq!(lines().count(), 6);
        for (i, line) in lines().enumerate() {
            let id = FuelLineId(i as u8);
            let at_valve = [line.valves[0].1 + Vec3::new(0.0, 0.9, 1.0)];
            let valve_wheel = valve_point(line.valves[0].1);
            // Shut: the clank from the feed pipe at the first Blaze, and the wheel's turn where the Player is.
            let shut = line_cues(line, LinePhase::Lit, SHUT, &at_valve);
            assert_eq!(cue_names(&shut), [Cue::PipeClank, Cue::ValveTurn], "{}", line.name);
            assert_eq!(shut[0].1, blaze_point(&line.blazes[0]));
            assert_eq!(shut[1].1, valve_wheel, "{}: the wheel the Player stands at", line.name);
            // Relight (a Shut line, a hand on its wheel): the wheel turns, no clank.
            let relight = line_cues(line, LinePhase::Shut { elapsed: SHUT_SECS }, FLARE, &at_valve);
            assert_eq!(cue_names(&relight), [Cue::ValveTurn], "{}", line.name);
            // Lit again: an ignite from each Blaze of the line, and no wheel.
            let ignite = line_cues(line, LinePhase::FlareUp { elapsed: FLARE_UP_SECS }, LinePhase::Lit, &at_valve);
            assert_eq!(ignite.len(), line.blazes.len(), "{}", line.name);
            assert!(ignite.iter().all(|&(cue, _)| cue == Cue::BlazeIgnite));
            assert!(line.blazes.iter().all(|blaze| ignite.iter().any(|&(_, at)| at == blaze_point(blaze))), "{}", line.name);
            // The hiss is a loop of the Flare-up, from every Blaze and Valve of this line only.
            let mut lines = FuelLines::start(6);
            lines.show(id, FLARE);
            let eye = Some(line.blazes[0].centre());
            let hiss: Vec<_> = wanted_loops(refinery(), &lines, eye).into_iter().filter(|(cue, ..)| *cue == Cue::GasHiss).collect();
            assert_eq!(hiss.len(), line.blazes.len() + line.valves.len(), "{}: a hiss from each Blaze and Valve", line.name);
            for blaze in line.blazes {
                assert!(hiss.iter().any(|&(_, _, at)| at == blaze_point(blaze)));
            }
            for &(_, at, _) in line.valves {
                assert!(hiss.iter().any(|&(_, _, here)| here == valve_point(at)));
            }
            // Lit and Shut have none of it.
            for phase in [LinePhase::Lit, SHUT] {
                let mut lines = FuelLines::start(6);
                lines.show(id, phase);
                assert!(wanted_loops(refinery(), &lines, eye).iter().all(|(cue, ..)| *cue != Cue::GasHiss), "{}", line.name);
            }
            // No other change sounds, and the seconds running on are no change.
            assert!(line_cues(line, LinePhase::Lit, FLARE, &at_valve).is_empty());
            assert!(line_cues(line, SHUT, LinePhase::Lit, &at_valve).is_empty());
            assert!(line_cues(line, FLARE, SHUT, &at_valve).is_empty());
            assert!(line_cues(line, LinePhase::Lit, LinePhase::Lit, &at_valve).is_empty());
        }
    }

    #[test]
    fn a_valve_turns_only_with_a_player_at_it() {
        let line = &refinery().fuel_lines[0];
        // No one about: a Boss Round's Shut or a Sabotaged line's relight, so the clank only.
        assert_eq!(cue_names(&line_cues(line, LinePhase::Lit, SHUT, &[])), [Cue::PipeClank]);
        assert!(line_cues(line, SHUT, FLARE, &[]).is_empty());
        // Someone across the Map is not at it; the nearer of two Valves is the one turned, whichever Player is there.
        let (a, b) = (line.valves[0].1, line.valves[1].1);
        assert_eq!(valve_used(line, &[Vec3::new(a.x + 40.0, a.y, a.z)]), None);
        assert_eq!(valve_used(line, &[Vec3::new(0.0, a.y + 20.0, 0.0), b + Vec3::X]), Some(valve_point(b)));
        // On another floor (a Player on the catwalk over the Valve) is not at it.
        assert_eq!(valve_used(line, &[a + Vec3::Y * 4.0]), None);
    }

    #[test]
    fn only_the_three_nearest_blaze_loops_play() {
        let layout = refinery();
        let lines = FuelLines::start(layout.fuel_lines.len());
        let blazes: Vec<(&Blaze, Vec3)> = layout.fuel_lines.iter().flat_map(|line| line.blazes).map(|blaze| (blaze, blaze_point(blaze))).collect();
        assert_eq!(blazes.len(), 7);
        // Standing in each Blaze in turn, and at the Gatehouse and on the Tower Deck.
        let mut eyes: Vec<Vec3> = blazes.iter().map(|&(_, at)| at + Vec3::Y * 1.1).collect();
        eyes.extend([Vec3::new(0.0, 1.6, 10.0), Vec3::new(0.0, 9.6, 40.0)]);
        for eye in eyes {
            let wanted = wanted_loops(layout, &lines, Some(eye));
            assert!(wanted.iter().all(|(cue, ..)| *cue == Cue::BlazeLoop), "every line is Lit, none flares");
            assert_eq!(wanted.len(), BLAZE_VOICES, "eye {eye}");
            let reach = wanted.iter().map(|(.., at)| at.distance(eye)).fold(0.0, f32::max);
            let nearer = blazes.iter().filter(|(_, at)| at.distance(eye) < reach).count();
            assert!(nearer < BLAZE_VOICES, "eye {eye}: {nearer} Blazes are nearer than the third");
            let mut keys: Vec<usize> = wanted.iter().map(|&(_, key, _)| key).collect();
            keys.dedup();
            assert_eq!(keys.len(), BLAZE_VOICES, "each its own emitter");
        }
        // A Shut line's Blazes are silent: with every line Shut, nothing crackles, and with one Lit it alone does.
        let mut shut = FuelLines::start(layout.fuel_lines.len());
        for i in 0..layout.fuel_lines.len() {
            shut.show(FuelLineId(i as u8), SHUT);
        }
        assert!(wanted_loops(layout, &shut, Some(Vec3::ZERO)).is_empty());
        shut.show(FuelLineId(3), LinePhase::Lit);
        let wanted = wanted_loops(layout, &shut, Some(Vec3::ZERO));
        assert_eq!(wanted.len(), layout.fuel_lines[3].blazes.len());
        assert!(layout.fuel_lines[3].blazes.iter().all(|blaze| wanted.iter().any(|&(.., at)| at == blaze_point(blaze))));
        // And nowhere to hear from, nothing plays.
        assert!(wanted_loops(layout, &lines, None).is_empty());
    }

    #[test]
    fn many_lines_flaring_together_hiss_from_the_nearest_eight() {
        let layout = refinery();
        let mut lines = FuelLines::start(layout.fuel_lines.len());
        for i in 0..layout.fuel_lines.len() {
            lines.show(FuelLineId(i as u8), FLARE);
        }
        let wanted = wanted_loops(layout, &lines, Some(Vec3::new(0.0, 1.6, 40.0)));
        assert_eq!(wanted.len(), HISS_VOICES);
        assert!(wanted.iter().all(|(cue, ..)| *cue == Cue::GasHiss));
    }
}
