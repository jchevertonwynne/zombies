//! The Run's counters that end up on the Run Record.

use bevy::prelude::*;

use crate::state::{GameState, Pause};
use crate::zombie::ZombieKilled;

#[derive(Resource, Default, Debug, PartialEq)]
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
        app.init_resource::<RunStats>()
            .add_systems(OnEnter(GameState::Playing), |mut stats: ResMut<RunStats>| {
                *stats = RunStats::default()
            })
            .add_systems(OnEnter(GameState::GameOver), count_final_down)
            .add_systems(Update, count_kills)
            .add_systems(Update, count_time.run_if(in_state(Pause::Running)));
    }
}

/// Without Quick Revive, the Down that ends the Run is the only kind.
pub fn count_final_down(mut stats: ResMut<RunStats>) {
    stats.downs = stats.downs.saturating_add(1);
}

/// `Time` here is virtual time, which stands still whenever the world is frozen.
fn count_time(time: Res<Time>, mut stats: ResMut<RunStats>) {
    stats.survived_secs += time.delta_secs_f64();
}

fn count_kills(mut kills: MessageReader<ZombieKilled>, mut stats: ResMut<RunStats>) {
    for kill in kills.read() {
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
