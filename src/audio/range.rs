//! The Archive's Ranges: what each one sounds like through its cycle (#542, from #512's "Sync for the cues").
//!
//! - `RangeRattle` loops through a Range's Warning and Waiting, from the middle of the Bookcases that are about to move.
//! - `RangeRoll` loops while it Swings, following the middle of the Bookcases that are moving.
//! - `ShelfCreak` sounds as a Swing starts, and `RangeThud` (with a `BookFall`) as the Bookcases come to Rest.
//! - `WheelTurn` sounds at the Wheel that was used, as the Range goes from Rest into its Warning.
//!
//! Every machine works all of it out from the `PiecePhase` it shows, as `ring_cues` does for a Bridge's clunk, so the Host, each client
//! and a late joiner (who is not given a thud or a turn for what they missed) hear the same Roll from where it is, and nothing is sent
//! on the wire. The dust and pages that go with a Rest (`maps::stacks_look`'s `RangeRested`) wait for the look's assets, and a headless
//! machine has none, so the sounds read the phase rather than that message.

use std::collections::HashMap;

use bevy::prelude::*;

use super::cues::Cue;
use super::{PlayCue, Sounds};
use crate::maps::map::{Layout, MapId};
use crate::maps::shifting::{Driver, Piece, PieceId, PiecePhase, PieceState};
use crate::maps::stacks_look::moved;
use crate::players::player::Player;
use crate::state::{GameState, InRun};

/// Whether `piece` is one of the Archive's Ranges: a Piece its Wheels roll.
pub fn is_range(piece: &Piece) -> bool {
    matches!(piece.driver, Driver::Wheels(_))
}

/// Whether Piece `id` of `layout` is a Range.
pub fn is_range_id(layout: &Layout, id: PieceId) -> bool {
    layout.shifting.get(usize::from(id.0)).is_some_and(is_range)
}

/// The middle of the Bookcases of `piece` that move between Rests `from` and `to`, with each at its pose in `poses`.
pub fn bookcases_centre(piece: &Piece, from: u8, to: u8, poses: &[f32]) -> Option<Vec3> {
    let moving = moved(piece, from, to);
    if moving.is_empty() {
        return None;
    }
    Some(moving.iter().map(|&i| piece.leaves[i].centre(poses[i])).sum::<Vec3>() / moving.len() as f32)
}

/// The loop a Range plays in `phase` and where from: the rattle through its Warning and Waiting, the roll through its Swing.
pub fn range_loop(piece: &Piece, phase: PiecePhase) -> Option<(Cue, Vec3)> {
    let cue = match phase {
        PiecePhase::Warning { .. } | PiecePhase::Waiting { .. } => Cue::RangeRattle,
        PiecePhase::Swinging { .. } => Cue::RangeRoll,
        PiecePhase::Rest(_) | PiecePhase::Cooling { .. } => return None,
    };
    let (from, to) = phase.heading()?;
    bookcases_centre(piece, from, to, &phase.poses(piece)).map(|at| (cue, at))
}

/// The one-shot cues a Range makes as it goes from `before` to `now`, each with where it sounds from: `ShelfCreak` as a Swing starts, the
/// `RangeThud` and a `BookFall` as one ends. `None` for either phase if it is not known, as at a first sight.
pub fn range_beats(piece: &Piece, before: PiecePhase, now: PiecePhase) -> Vec<(Cue, Vec3)> {
    let mut beats = Vec::new();
    match (before, now) {
        (PiecePhase::Swinging { from, to, .. }, now) if !now.swinging() => {
            let rest = now.standing().unwrap_or(to);
            if let Some(at) = bookcases_centre(piece, from, to, &piece.rests[usize::from(rest)].at) {
                beats.push((Cue::RangeThud, at));
                beats.push((Cue::BookFall, at + Vec3::Y * 1.5));
            }
        }
        (before, PiecePhase::Swinging { from, to, .. }) if !before.swinging() => {
            if let Some(at) = bookcases_centre(piece, from, to, &now.poses(piece)) {
                beats.push((Cue::ShelfCreak, at));
            }
        }
        _ => {}
    }
    beats
}

/// The Wheel of `piece` a Player used as it went from Rest into `now`: of the Wheels that roll to the Rest it is heading for, the one a
/// Player stands nearest (a use is only made from beside the Wheel), or the first if no one is about.
pub fn wheel_used(piece: &Piece, to: u8, players: &[Vec3]) -> Option<Vec3> {
    let wheels = piece.wheels().iter().filter(|wheel| wheel.rest == to);
    wheels
        .map(|wheel| {
            let nearest = players.iter().map(|player| player.distance_squared(wheel.at)).fold(f32::INFINITY, f32::min);
            (wheel.at + Vec3::Y * 1.1, nearest)
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(at, _)| at)
}

/// The cues, `beats` and the Wheel's turn, a Range makes as it goes from `before` to `now`.
pub fn range_cues_for(piece: &Piece, before: PiecePhase, now: PiecePhase, players: &[Vec3]) -> Vec<(Cue, Vec3)> {
    let mut cues = range_beats(piece, before, now);
    if let (PiecePhase::Rest(_), PiecePhase::Warning { to, .. }) = (before, now)
        && let Some(at) = wheel_used(piece, to, players)
    {
        cues.push((Cue::WheelTurn, at));
    }
    cues
}

/// Asks for each Range's one-shot cues as its phase changes, on every machine from the phase it shows.
pub fn range_cues(
    pieces: Query<&PieceState>,
    players: Query<&Transform, With<Player>>,
    map: Option<Res<MapId>>,
    mut was: Local<HashMap<PieceId, PiecePhase>>,
    mut cues: MessageWriter<PlayCue>,
) {
    let Some(map) = map else { return };
    let players: Vec<Vec3> = players.iter().map(|transform| transform.translation).collect();
    for state in &pieces {
        let Some(piece) = map.layout().shifting.get(usize::from(state.id().0)).filter(|piece| is_range(piece)) else { continue };
        let now = state.phase();
        let before = was.insert(state.id(), now);
        // Only a change the machine has watched makes a sound: a phase first seen (a late joiner's) does not.
        let Some(before) = before else { continue };
        for (cue, at) in range_cues_for(piece, before, now, &players) {
            cues.write(PlayCue::new(cue, Some(at)));
        }
    }
}

/// A Range's rattle or roll loop, following the Bookcases it sounds from.
#[derive(Component)]
pub(super) struct RangeLoop {
    piece: PieceId,
    cue: Cue,
}

/// Keeps the rattle and the roll looping from the Bookcases of each Range that warns or rolls, and none from any other.
pub(super) fn range_loops(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    pieces: Query<&PieceState>,
    map: Option<Res<MapId>>,
    state: Res<State<GameState>>,
    mut playing: Query<(Entity, &RangeLoop, &mut Transform)>,
) {
    let wanted: Vec<(PieceId, Cue, Vec3)> = match (&map, *state.get() == GameState::Playing) {
        (Some(map), true) => pieces
            .iter()
            .filter_map(|state| {
                let piece = map.layout().shifting.get(usize::from(state.id().0)).filter(|piece| is_range(piece))?;
                range_loop(piece, state.phase()).map(|(cue, at)| (state.id(), cue, at))
            })
            .collect(),
        _ => Vec::new(),
    };
    for (entity, sound, mut transform) in &mut playing {
        match wanted.iter().find(|&&(piece, cue, _)| piece == sound.piece && cue == sound.cue) {
            Some(&(.., at)) => transform.translation = at,
            None => commands.entity(entity).despawn(),
        }
    }
    for (piece, cue, at) in wanted {
        if playing.iter().any(|(_, sound, _)| sound.piece == piece && sound.cue == cue) {
            continue;
        }
        if let Some(sound) = sounds.looping(cue, 1.0) {
            commands.spawn((InRun, RangeLoop { piece, cue }, sound)).insert(Transform::from_translation(at));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tuning::SWING_SECS;

    fn ranges() -> Vec<&'static Piece> {
        MapId::Archive.layout().shifting.iter().filter(|piece| is_range(piece)).collect()
    }

    fn cues_of(piece: &Piece, before: PiecePhase, now: PiecePhase) -> Vec<Cue> {
        range_cues_for(piece, before, now, &[]).into_iter().map(|(cue, _)| cue).collect()
    }

    /// The rattle through Warning and Waiting, the roll while Swinging, the thud at Rest: each only in its own phase, for every Range, from
    /// the Bookcases that move.
    #[test]
    fn range_cues_follow_the_phase() {
        let layout = MapId::Archive.layout();
        assert_eq!(ranges().len(), 5, "the four Ranges and the Great Range");
        for piece in ranges() {
            for from in 0..piece.rests.len() as u8 {
                for to in (0..piece.rests.len() as u8).filter(|&to| to != from && !moved(piece, from, to).is_empty()) {
                    let loop_cue = |phase| range_loop(piece, phase).map(|(cue, _)| cue);
                    // Rattle: through Warning and through Waiting, and no roll.
                    assert_eq!(loop_cue(PiecePhase::Warning { from, to, elapsed: 0.0 }), Some(Cue::RangeRattle));
                    assert_eq!(loop_cue(PiecePhase::Warning { from, to, elapsed: 1.5 }), Some(Cue::RangeRattle));
                    assert_eq!(loop_cue(PiecePhase::Waiting { from, to }), Some(Cue::RangeRattle));
                    // Roll: through the whole Swing, the rattle gone.
                    for elapsed in [0.0, SWING_SECS / 2.0, SWING_SECS] {
                        assert_eq!(loop_cue(PiecePhase::Swinging { from, to, elapsed }), Some(Cue::RangeRoll), "{from}->{to} at {elapsed}");
                    }
                    // Quiet at Rest and while it cools.
                    assert_eq!(loop_cue(PiecePhase::Rest(to)), None);
                    assert_eq!(loop_cue(PiecePhase::Cooling { at: to, elapsed: 1.0 }), None);
                    // Thud: as the Swing ends, with the fall of a book, from the Bookcases at their new Rest; none for any other change.
                    let swing = PiecePhase::Swinging { from, to, elapsed: SWING_SECS };
                    assert_eq!(cues_of(piece, swing, PiecePhase::Cooling { at: to, elapsed: 0.0 }), [Cue::RangeThud, Cue::BookFall]);
                    assert_eq!(cues_of(piece, swing, PiecePhase::Rest(to)), [Cue::RangeThud, Cue::BookFall]);
                    assert!(cues_of(piece, swing, swing).is_empty(), "mid-Swing is not a Rest");
                    assert!(cues_of(piece, PiecePhase::Cooling { at: to, elapsed: 3.0 }, PiecePhase::Rest(to)).is_empty());
                    assert!(cues_of(piece, PiecePhase::Warning { from, to, elapsed: 1.0 }, PiecePhase::Waiting { from, to }).is_empty());
                    // A creak as the Swing starts.
                    assert_eq!(cues_of(piece, PiecePhase::Waiting { from, to }, PiecePhase::Swinging { from, to, elapsed: 0.0 }), [Cue::ShelfCreak]);
                    // The thud sounds where the Bookcases came to rest: at their height over the floor they stand on, not at the origin.
                    let at = range_beats(piece, swing, PiecePhase::Rest(to))[0].1;
                    assert!((at.y - piece.leaves[0].hinge.y).abs() < 6.0 && at.xz().length() > 1.0, "{at}");
                }
            }
        }
        // The roll follows the Bookcases: it moves with them through the Swing.
        let piece = ranges()[0];
        let (from, to) = (0, 1);
        let start = range_loop(piece, PiecePhase::Swinging { from, to, elapsed: 0.0 }).unwrap().1;
        let end = range_loop(piece, PiecePhase::Swinging { from, to, elapsed: SWING_SECS }).unwrap().1;
        assert!(start.distance(end) > 0.5, "the roll travels {start} to {end}");
        // None of it is a Bridge's: Ranges are the only Pieces here, and the Archive has no other kind.
        assert!(layout.shifting.iter().all(is_range));
    }

    /// A use of a Wheel turns the one a Player stands beside, for the Rest it opens, and only as a Range leaves Rest.
    #[test]
    fn wheel_turn_sounds_at_the_wheel_that_was_used() {
        let piece = ranges()[0];
        let rest = 0;
        let wheels: Vec<_> = piece.wheels().iter().filter(|wheel| wheel.rest == rest).collect();
        assert_eq!(wheels.len(), 2, "a Wheel at each mouth of the Aisle");
        let from = piece.start;
        assert_ne!(from, rest);
        let warning = PiecePhase::Warning { from, to: rest, elapsed: 0.0 };
        for wheel in &wheels {
            let at = range_cues_for(piece, PiecePhase::Rest(from), warning, &[wheel.at + Vec3::new(0.3, 1.0, 0.3)]);
            let turn: Vec<_> = at.iter().filter(|(cue, _)| *cue == Cue::WheelTurn).collect();
            assert_eq!(turn.len(), 1);
            assert!(turn[0].1.xz().distance(wheel.at.xz()) < 0.01, "the turn is at the Wheel stood beside");
        }
        // Not as a Warning goes on, nor from a Range that is not at Rest.
        assert!(!cues_of(piece, warning, PiecePhase::Waiting { from, to: rest }).contains(&Cue::WheelTurn));
        assert!(!cues_of(piece, PiecePhase::Cooling { at: from, elapsed: 1.0 }, warning).contains(&Cue::WheelTurn));
        // With no one about it still turns, at one of the two.
        assert!(cues_of(piece, PiecePhase::Rest(from), warning).contains(&Cue::WheelTurn));
    }

    #[test]
    fn the_new_cues_are_heard_from_where_they_are_and_the_tone_and_bed_flat() {
        use super::super::cues::Mix;
        for cue in [Cue::RangeRattle, Cue::RangeRoll, Cue::RangeThud, Cue::WheelTurn, Cue::Drip, Cue::PageRustle, Cue::ShelfCreak, Cue::BookFall] {
            assert_eq!(cue.spec().mix, Mix::Spatial, "{cue:?}");
        }
        for cue in [Cue::ArchiveTone, Cue::CellarBed] {
            assert_eq!(cue.spec().mix, Mix::Flat, "{cue:?}");
        }
        let gains = [
            (Cue::RangeRattle, 0.8),
            (Cue::RangeRoll, 0.9),
            (Cue::RangeThud, 1.0),
            (Cue::WheelTurn, 0.7),
            (Cue::ArchiveTone, 0.22),
            (Cue::CellarBed, 0.25),
            (Cue::Drip, 0.35),
            (Cue::PageRustle, 0.3),
            (Cue::ShelfCreak, 0.3),
            (Cue::BookFall, 0.5),
        ];
        for (cue, gain) in gains {
            assert_eq!(cue.spec().gain, gain, "{cue:?}");
        }
    }
}
