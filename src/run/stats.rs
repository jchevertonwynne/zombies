//! The Run's counters that end up on the Run Record.

use bevy::prelude::*;

use crate::players::last_stand::{Down, go_down};
use crate::players::revive::Revived;
use crate::players::player::Player;
use crate::state::world_runs;
use crate::enemies::zombie::ZombieKilled;

/// A Player's counters for the Run.
#[derive(Component, Default, Debug, PartialEq)]
pub struct RunStats {
    pub kills: u32,
    pub headshots: u32,
    pub downs: u32,
    /// Teammates this Player has got back up from a Last Stand.
    pub revives: u32,
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
            .add_systems(Update, (count_kills, count_downs.after(go_down), count_revives))
            .add_systems(Update, count_time.run_if(world_runs));
    }
}

/// Every Down counts: one ending in a Self-Revive as much as the one ending the Run.
fn count_downs(mut downs: MessageReader<Down>, mut stats: Query<&mut RunStats, With<Player>>) {
    for down in downs.read() {
        let Ok(mut stats) = stats.get_mut(down.player) else {
            continue;
        };
        stats.downs = stats.downs.saturating_add(1);
    }
}

/// Each Revive counts for the Player who did it.
fn count_revives(mut revived: MessageReader<Revived>, mut stats: Query<&mut RunStats, With<Player>>) {
    for revive in revived.read() {
        if let Ok(mut stats) = stats.get_mut(revive.reviver) {
            stats.revives = stats.revives.saturating_add(1);
        }
    }
}

/// `Time` here is virtual time, which stands still whenever the world is frozen.
fn count_time(time: Res<Time>, mut stats: Query<&mut RunStats, With<Player>>) {
    for mut stats in &mut stats {
        stats.survived_secs += time.delta_secs_f64();
    }
}

/// Each kill counts for the Player whose attack made it.
fn count_kills(mut kills: MessageReader<ZombieKilled>, mut stats: Query<&mut RunStats, With<Player>>) {
    for kill in kills.read() {
        let Some(Ok(mut stats)) = kill.player.map(|player| stats.get_mut(player)) else {
            continue;
        };
        stats.record_kill(kill);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enemies::zombie::{Attack, HitZone, Pay};

    fn kill(pay: Pay, headshot: bool) -> ZombieKilled {
        ZombieKilled {
            player: None,
            pay,
            headshot,
            zone: if headshot { HitZone::Head } else { HitZone::Torso },
            attack: Attack::Bullet,
            at: bevy::math::Vec3::ZERO,
            surge: false,
            zombie: Entity::PLACEHOLDER,
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
            revives: 0,
            survived_secs: 0.0,
        };
        stats.record_kill(&kill(Pay::Paid, true));
        assert_eq!(stats.kills, u32::MAX);
        assert_eq!(stats.headshots, u32::MAX);
    }

    #[test]
    fn kills_and_downs_count_for_the_player_they_name() {
        use crate::players::last_stand::DownOutcome;
        let mut app = App::new();
        app.add_message::<ZombieKilled>()
            .add_message::<Down>()
            .add_message::<Revived>()
            .add_systems(Update, (count_kills, count_downs, count_revives));
        let first = app.world_mut().spawn((Player, RunStats::default())).id();
        let second = app.world_mut().spawn((Player, RunStats::default())).id();
        app.world_mut().write_message(ZombieKilled { player: Some(second), ..kill(Pay::Paid, true) });
        app.world_mut().write_message(ZombieKilled { player: None, ..kill(Pay::Paid, false) });
        app.world_mut().write_message(Down { player: second, outcome: DownOutcome::LastStand });
        app.world_mut().write_message(Revived { player: second, reviver: first });
        app.update();
        let stats = |player| {
            let stats = app.world().get::<RunStats>(player).unwrap();
            (stats.kills, stats.headshots, stats.downs, stats.revives)
        };
        assert_eq!(stats(first), (0, 0, 0, 1));
        assert_eq!(stats(second), (1, 1, 1, 0));
    }
}
