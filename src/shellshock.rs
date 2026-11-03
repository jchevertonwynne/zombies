//! Shellshock: a blast close by leaves the Player dazed for a few seconds, with a blurred view, a
//! muffled mix and ×`SHELLSHOCK_SPEED_MULT` move speed. Anything can Shellshock the Player by
//! writing a [`Shellshock`] message: a Nova Crawler's gas, a Napalm's explosion, a Teleporter
//! aftereffect.

use bevy::prelude::*;

use crate::hud::overlay::{Overlay, Overlays};
use crate::modifier::{Effect, Modifier, Modifiers, Source, Stat};
use crate::player::Player;
use crate::state::Pause;
use crate::tuning::*;

/// Where the Shellshock's move speed entry comes from.
const SHELLSHOCK_SOURCE: Source = Source("Shellshock");

/// Shellshocks the Player for `secs`. A longer Shellshock already under way is left be.
#[derive(Message, Debug, Clone, Copy)]
pub struct Shellshock {
    pub secs: f32,
}

/// A Shellshock began, with the Player not already Shellshocked.
#[derive(Message, Debug, Clone, Copy)]
pub struct ShellshockStarted;

/// How long a Player stays Shellshocked.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq)]
pub struct Shellshocked {
    left: f32,
}

impl Shellshocked {
    /// Shellshocks the Player for `secs`, unless they already have longer left. True if they
    /// weren't Shellshocked before.
    pub fn shock(&mut self, secs: f32) -> bool {
        let started = !self.active();
        self.left = self.left.max(secs);
        started && self.active()
    }

    /// Runs it down by `dt`. True on the tick it wears off.
    pub fn tick(&mut self, dt: f32) -> bool {
        if !self.active() {
            return false;
        }
        self.left = (self.left - dt).max(0.0);
        !self.active()
    }

    pub fn active(&self) -> bool {
        self.left > 0.0
    }
}

pub struct ShellshockPlugin;

impl Plugin for ShellshockPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Shellshock>()
            .add_message::<ShellshockStarted>()
            .add_systems(Update, shellshock_player.run_if(in_state(Pause::Running)));
    }
}

/// Starts and ends Shellshocks: the overlay pulses for the whole of each, and the Player's move
/// speed entry lasts while it does.
fn shellshock_player(
    mut shocks: MessageReader<Shellshock>,
    mut started: MessageWriter<ShellshockStarted>,
    mut overlays: ResMut<Overlays>,
    player: Single<(&mut Shellshocked, &mut Modifiers), With<Player>>,
    time: Res<Time>,
) {
    let (mut shocked, mut modifiers) = player.into_inner();
    if shocked.tick(time.delta_secs()) {
        modifiers
            .0
            .retain(|entry| entry.source != SHELLSHOCK_SOURCE);
    }
    for shock in shocks.read() {
        overlays.pulse(Overlay::Shellshock, SHELLSHOCK_OVERLAY_STRENGTH, shock.secs);
        if shocked.shock(shock.secs) {
            started.write(ShellshockStarted);
            modifiers.0.push(Modifier {
                stat: Stat::MoveSpeed,
                source: SHELLSHOCK_SOURCE,
                effect: Effect::Mult(SHELLSHOCK_SPEED_MULT),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shellshock_lasts_its_seconds_then_wears_off_once() {
        let mut shocked = Shellshocked::default();
        assert!(!shocked.active());
        assert!(shocked.shock(2.5));
        assert!(!shocked.tick(2.0));
        assert!(shocked.active());
        assert!(shocked.tick(1.0), "it wears off on this tick");
        assert!(!shocked.active());
        assert!(!shocked.tick(1.0), "and only once");
    }

    #[test]
    fn a_second_shellshock_extends_but_never_shortens_it() {
        let mut shocked = Shellshocked::default();
        assert!(shocked.shock(3.0));
        assert!(!shocked.shock(1.0), "already Shellshocked: no new start");
        assert!(!shocked.tick(2.5), "the shorter one didn't cut it short");
        assert!(!shocked.shock(2.0));
        assert!(!shocked.tick(1.5), "the longer one extended it");
        assert!(shocked.tick(1.0));
    }

    #[test]
    fn a_zero_length_shellshock_does_nothing() {
        let mut shocked = Shellshocked::default();
        assert!(!shocked.shock(0.0));
        assert!(!shocked.active());
    }

    #[test]
    fn shellshock_halves_move_speed_through_the_seam() {
        let entry = Modifier {
            stat: Stat::MoveSpeed,
            source: SHELLSHOCK_SOURCE,
            effect: Effect::Mult(SHELLSHOCK_SPEED_MULT),
        };
        assert_eq!(
            crate::modifier::resolve(Stat::MoveSpeed, 5.0, [&entry]),
            2.5
        );
    }
}
