//! The Run's counters that end up on the Run Record.

use bevy::prelude::*;

use crate::last_stand::{Down, go_down};
use crate::player::{LocalPlayer, Player};
use crate::state::Pause;
use crate::zombie::ZombieKilled;

/// A Player's counters for the Run.
#[derive(Component, Default, Debug, PartialEq)]
pub struct RunStats {
    pub kills: u32,
    pub headshots: u32,
    pub downs: u32,
    /// Game-time survived in seconds. Only counts while the world runs, so pauses don't.
    pub survived_secs: f64,
}

impl RunStats {
    /// Counts a kill. Every kill counts, Paid or Unpaid.
    pub fn record_kill(&mut self, kill: &ZombieKilled) {
        self.kills = self.kills.saturating_add(1);
        if kill.headshot {
            self.headshots = self.headshots.saturating_add(1);
        }
    }
}

pub struct StatsPlugin;

impl Plugin for StatsPlugin {
    fn build(&self, app: &mut App) {
        app
            // Counted in the frame of the Down, so the one that ends the Run is in its Record.
            .add_systems(Update, (count_kills, count_downs.after(go_down)))
            .add_systems(Update, count_time.run_if(in_state(Pause::Running)));
    }
}

/// Every Down counts: one ending in a Self-Revive as much as the one ending the Run.
fn count_downs(mut downs: MessageReader<Down>, mut stats: Query<&mut RunStats, With<LocalPlayer>>) {
    for _ in downs.read() {
        let Ok(mut stats) = stats.single_mut() else {
            continue;
        };
        stats.downs = stats.downs.saturating_add(1);
    }
}

/// `Time` here is virtual time, which stands still whenever the world is frozen.
fn count_time(time: Res<Time>, mut stats: Query<&mut RunStats, With<Player>>) {
    for mut stats in &mut stats {
        stats.survived_secs += time.delta_secs_f64();
    }
}

/// Kills count for the `LocalPlayer`, who lands them all until kills name their killer.
fn count_kills(mut kills: MessageReader<ZombieKilled>, mut stats: Query<&mut RunStats, With<LocalPlayer>>) {
    for kill in kills.read() {
        let Ok(mut stats) = stats.single_mut() else {
            continue;
        };
        stats.record_kill(kill);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zombie::{Attack, HitZone, Pay};

    fn kill(pay: Pay, headshot: bool) -> ZombieKilled {
        ZombieKilled {
            pay,
            headshot,
            zone: if headshot { HitZone::Head } else { HitZone::Torso },
            attack: Attack::Bullet,
            at: bevy::math::Vec3::ZERO,
        }
    }

    #[test]
    fn paid_and_unpaid_kills_both_count() {
        let mut stats = RunStats::default();
        stats.record_kill(&kill(Pay::Paid, false));
        stats.record_kill(&kill(Pay::Unpaid, false));
        assert_eq!(stats.kills, 2);
        assert_eq!(stats.headshots, 0);
    }

    #[test]
    fn headshot_kills_count_as_headshots_too() {
        let mut stats = RunStats::default();
        stats.record_kill(&kill(Pay::Paid, true));
        stats.record_kill(&kill(Pay::Unpaid, true));
        assert_eq!(stats.kills, 2);
        assert_eq!(stats.headshots, 2);
    }

    #[test]
    fn counters_saturate_instead_of_wrapping() {
        let mut stats = RunStats {
            kills: u32::MAX,
            headshots: u32::MAX,
            downs: 0,
            survived_secs: 0.0,
        };
        stats.record_kill(&kill(Pay::Paid, true));
        assert_eq!(stats.kills, u32::MAX);
        assert_eq!(stats.headshots, u32::MAX);
    }
}
