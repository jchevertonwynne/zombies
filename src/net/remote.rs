//! The Host's buffer of a remote Player's input. Adapted from Block Racers' `Remote`.
//!
//! A remote Player's Player is stepped by one of their ticks of input for each of the Host's, so
//! that it is where their own game, which stepped it by the same input, has it. A tick that
//! hasn't come when it is wanted is stepped as the one before was, and given up when it comes:
//! waited for, the Player would be a tick behind their game for the rest of the Run, and put back
//! by a tick's travel when it was found out. A few ticks are kept in hand so that this is seldom,
//! and how many is changed only when the connection has changed (input comes too late time after
//! time, or a little late now and then, or more is waiting than is ever needed), which is the one
//! time it puts the Player right. The Host never rewinds.

use std::collections::VecDeque;

use bevy::prelude::*;

use super::protocol::Peer;
use super::state::{Step, Tick};
use crate::tuning::{REMOTE_INPUT_DEPTH as DEPTH, REMOTE_INPUT_WINDOW as WINDOW, REMOTE_STALL_TICKS};

/// A run of ticks that didn't come in time no longer than this is the connection being uneven,
/// which one more in hand would have covered; a longer one is the connection or their game
/// stopping for a moment, which no number in hand would have.
const SHORT: u32 = 2;
/// How many times running the newest tick may come too late to be stepped by before the ticks
/// are taken in hand again.
const LATE: u32 = 6;

/// A Player driven by someone else's game.
#[derive(Component)]
pub struct Remote {
    pub peer: Peer,
    /// The ticks heard of and not yet stepped by, oldest first.
    queue: VecDeque<(Tick, Step)>,
    newest: Option<Tick>,
    /// A tick newer than any before has been heard since the Player was last stepped.
    fresh: bool,
    /// The last of their ticks the Player has been stepped by, or in the place of; none until
    /// one has been heard.
    used: Option<Tick>,
    /// Ticks to step as before without taking one of theirs, so that more are in hand.
    hold: Tick,
    /// A Jump pressed in a tick that was never stepped by, for the next that is.
    owed: bool,
    /// How many steps running have had none of theirs to step by.
    starved: u32,
    /// How many of their ticks running didn't come in time, and how many times running the
    /// newest heard came too late to be stepped by.
    missed: u32,
    late: u32,
    /// What has been seen of the ticks in hand over the steps counted so far: the fewest there
    /// were, and how many short runs didn't come in time.
    counted: u32,
    fewest: i64,
    short: u32,
}

/// What a Player is stepped by for a tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Next {
    /// Nothing has been heard of them: the Player is left as they are.
    Nothing,
    /// Their next tick.
    Step(Step),
    /// The tick before again, with no Jump newly pressed: their next hasn't come.
    Missing,
    /// The tick before again, while their ticks are taken in hand.
    Held,
}

impl Remote {
    pub fn new(peer: Peer) -> Self {
        Remote {
            peer,
            queue: VecDeque::new(),
            newest: None,
            fresh: false,
            used: None,
            hold: 0,
            owed: false,
            starved: 0,
            missed: 0,
            late: 0,
            counted: 0,
            fewest: i64::MAX,
            short: 0,
        }
    }

    /// Nothing has been heard of their pressing for too long, and they stand still until
    /// something is.
    pub fn stalled(&self) -> bool {
        self.starved >= REMOTE_STALL_TICKS
    }

    /// The last of their ticks the Player has been stepped by, or in the place of.
    pub fn used(&self) -> Tick {
        self.used.unwrap_or(0)
    }

    /// A Jump pressed in a tick that won't be stepped by is pressed in the next that is.
    fn owe(&mut self, step: &Step) {
        self.owed |= step.jump();
    }

    /// Takes in what their game says it pressed, less what was heard before.
    pub fn hear(&mut self, last: Tick, steps: &[Step]) {
        let count = steps.len() as Tick;
        for (i, &step) in steps.iter().enumerate() {
            let Some(tick) = (last + 1 + i as Tick).checked_sub(count) else {
                continue;
            };
            if self.newest.is_some_and(|newest| tick <= newest) {
                continue;
            }
            (self.newest, self.fresh) = (Some(tick), true);
            // Too late: the Player has been stepped in its place.
            if self.used.is_some_and(|used| tick <= used) {
                self.owe(&step);
            } else {
                self.queue.push_back((tick, step));
            }
        }
    }

    /// What the Player is to be stepped by for the next tick, counting it towards `stalled`.
    pub fn next(&mut self) -> Next {
        let next = self.take();
        match next {
            Next::Step(_) => self.starved = 0,
            Next::Missing => self.starved += 1,
            Next::Nothing | Next::Held => {}
        }
        next
    }

    fn take(&mut self) -> Next {
        let Some(newest) = self.newest else {
            return Next::Nothing;
        };
        let used = match self.used {
            Some(used) => used,
            None => {
                // Stepping begins with the first heard and the one before it in hand, or, the
                // first being their first, after a tick's wait.
                let used = newest.saturating_sub(DEPTH);
                (self.used, self.hold) = (Some(used), DEPTH - (newest - used));
                used
            }
        };
        self.watch(newest, i64::from(newest) - i64::from(used));
        if self.hold > 0 {
            self.hold -= 1;
            return Next::Held;
        }
        let next = self.used() + 1;
        self.used = Some(next);
        // Ticks passed over are not stepped by, but what was pressed in them was.
        while let Some(&(tick, step)) = self.queue.front()
            && tick < next
        {
            self.queue.pop_front();
            self.owe(&step);
        }
        match self.queue.front() {
            Some(&(tick, step)) if tick == next => {
                self.queue.pop_front();
                let step = step.with_jump(step.jump() || self.owed);
                self.owed = false;
                if (1..=SHORT).contains(&self.missed) {
                    self.short += 1;
                }
                self.missed = 0;
                Next::Step(step)
            }
            _ => {
                self.missed += 1;
                Next::Missing
            }
        }
    }

    /// Keeps more or fewer of their ticks in hand, `spare` being how many there are now, the next
    /// among them.
    fn watch(&mut self, newest: Tick, spare: i64) {
        let fresh = std::mem::take(&mut self.fresh);
        if spare >= 1 {
            self.late = 0;
        } else if fresh {
            self.late += 1;
        }
        if self.late >= LATE {
            // Ticks keep coming, and too late: their game is behind (it stopped for longer than
            // it could make up). The Player is stepped as before while they are taken in hand
            // from the newest on.
            debug!("player {}: ticks {} late, taken in hand again", self.peer, -spare + 1);
            (self.used, self.hold, self.late) = (Some(newest), DEPTH, 0);
            (self.counted, self.fewest, self.short) = (0, i64::MAX, 0);
            return;
        }
        self.fewest = self.fewest.min(spare);
        self.counted += 1;
        if self.counted < WINDOW {
            return;
        }
        if self.short >= 2 {
            // Ticks come a little late now and then: one more is kept in hand.
            debug!("player {}: ticks a little late {} times, one more kept in hand", self.peer, self.short);
            self.hold += 1;
        } else if self.fewest > i64::from(DEPTH) {
            // More have been waiting all along than were ever wanted: the Player is stepped on
            // past those it doesn't need.
            let past = (self.fewest - i64::from(DEPTH)) as Tick;
            debug!("player {}: {past} ticks more in hand than wanted, stepped past", self.peer);
            self.used = Some(self.used() + past);
        }
        (self.counted, self.fewest, self.short) = (0, i64::MAX, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tick whose yaw is its own number, to tell it by.
    fn step(tick: Tick) -> Step {
        Step { yaw: tick as f32, ..default() }
    }

    /// Hears a tick, with the three before it, as their game sends it.
    fn hear(remote: &mut Remote, tick: Tick) {
        let steps: Vec<Step> = (tick.saturating_sub(3).max(1)..=tick).map(step).collect();
        remote.hear(tick, &steps);
    }

    /// Which of their ticks the Player is stepped by, if any.
    fn driven(remote: &mut Remote) -> Option<Tick> {
        match remote.next() {
            Next::Step(step) => Some(step.yaw as Tick),
            _ => None,
        }
    }

    #[test]
    fn ticks_are_heard_once_and_in_order() {
        let mut remote = Remote::new(1);
        remote.hear(3, &[step(1), step(2), step(3)]);
        remote.hear(5, &[step(2), step(3), step(4), step(5)]);
        let waiting: Vec<Tick> = remote.queue.iter().map(|(tick, _)| *tick).collect();
        assert_eq!(waiting, [1, 2, 3, 4, 5]);
        // A late one that has been overtaken is ignored.
        remote.hear(3, &[step(9)]);
        assert_eq!(remote.queue.len(), 5);
    }

    /// The Player is stepped by one of their ticks for each of the Host's. One held up on its way
    /// is stepped in the place of, and those after it in their turn: the Player is never a tick
    /// behind their game, nor ahead of it.
    #[test]
    fn a_tick_held_up_is_stepped_in_the_place_of() {
        let mut remote = Remote::new(1);
        hear(&mut remote, 4);
        // Two in hand, the next among them.
        assert_eq!(driven(&mut remote), Some(3));
        hear(&mut remote, 5);
        assert_eq!(driven(&mut remote), Some(4));
        assert_eq!(driven(&mut remote), Some(5));
        // Ticks 6 to 9 are held up, and come together.
        assert_eq!(driven(&mut remote), None);
        assert_eq!(driven(&mut remote), None);
        assert_eq!(remote.used(), 7);
        hear(&mut remote, 9);
        assert_eq!(driven(&mut remote), Some(8));
        hear(&mut remote, 10);
        assert_eq!(driven(&mut remote), Some(9));
    }

    #[test]
    fn a_jump_pressed_in_a_tick_given_up_is_still_pressed() {
        let mut remote = Remote::new(1);
        hear(&mut remote, 3);
        assert_eq!(driven(&mut remote), Some(2));
        assert_eq!(driven(&mut remote), Some(3));
        assert_eq!(driven(&mut remote), None);
        let jump = step(4).with_jump(true);
        remote.hear(5, &[jump, step(5)]);
        let Next::Step(step) = remote.next() else { panic!("tick 5 should have been stepped by") };
        assert_eq!((step.yaw, step.jump()), (5.0, true));
    }

    /// Two little gaps in a second, each of which leaves a tick to be stepped in the place of,
    /// and one more is kept in hand: a third like them leaves none.
    #[test]
    fn ticks_a_little_late_now_and_then_have_one_more_kept_in_hand() {
        let mut remote = Remote::new(1);
        let (mut heard, mut missed) = (0, Vec::new());
        for at in 0..WINDOW + 40 {
            // At each gap two ticks' pressing is a tick late, and comes with the next.
            let gap = [10, 30, WINDOW + 20].iter().find(|&&gap| (gap..gap + 3).contains(&at));
            let coming = match gap {
                Some(&gap) if at < gap + 2 => 0,
                Some(_) => 3,
                None => 1,
            };
            for _ in 0..coming {
                heard += 1;
                hear(&mut remote, heard);
            }
            if remote.next() == Next::Missing {
                missed.push(at);
            }
        }
        assert_eq!(missed, [11, 31]);
    }

    /// Their game stopping for longer than it could make up has its ticks all come too late,
    /// until they are taken in hand again.
    #[test]
    fn ticks_that_keep_coming_too_late_are_taken_in_hand_again() {
        let mut remote = Remote::new(1);
        hear(&mut remote, 4);
        for _ in 0..20 {
            driven(&mut remote);
        }
        let mut by: Vec<(Tick, Option<Tick>)> = Vec::new();
        for heard in 5..5 + LATE + DEPTH + 3 {
            hear(&mut remote, heard);
            by.push((heard, driven(&mut remote)));
        }
        // Given up until one has come too late too many times, then held back while as many are
        // taken in hand as at first.
        let (given_up, again) = by.split_at((LATE + DEPTH - 1) as usize);
        assert!(given_up.iter().all(|(_, driven)| driven.is_none()), "{by:?}");
        assert!(again.iter().all(|(heard, driven)| *driven == Some(heard - 1)), "{by:?}");
    }

    /// Their game getting further ahead of the Host than it needs to be, and staying there, has
    /// the ticks it needn't have in hand stepped past.
    #[test]
    fn ticks_always_waiting_in_numbers_are_stepped_past() {
        let mut remote = Remote::new(1);
        let mut heard = 0;
        for at in 0..2 * WINDOW {
            for _ in 0..if at < 8 { 2 } else { 1 } {
                heard += 1;
                hear(&mut remote, heard);
            }
            driven(&mut remote);
        }
        assert_eq!(heard - remote.used(), DEPTH - 1);
    }

    #[test]
    fn a_player_whose_ticks_stop_coming_is_stalled_until_one_does() {
        let mut remote = Remote::new(1);
        hear(&mut remote, 4);
        for _ in 0..REMOTE_STALL_TICKS + 10 {
            remote.next();
        }
        assert!(remote.stalled());
        let now = remote.used() + 3;
        hear(&mut remote, now);
        for _ in 0..4 {
            remote.next();
        }
        assert!(!remote.stalled(), "a tick came and the Player is still stalled");
    }
}
