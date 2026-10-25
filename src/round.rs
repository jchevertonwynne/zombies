use std::ops::RangeInclusive;

use bevy::prelude::*;

use crate::map::SpawnPoint;
use crate::state::{GameState, Pause};
use crate::tuning::*;
use crate::zombie::{Dying, SpeedTier, Zombie, ZombieAssetsParam, spawn_zombie};
use crate::zone::ActiveZones;

/// Zombie health for a Round, built Round by Round as BO1 does: +100 a Round through Round 9,
/// then +10% (floored) a Round, with no cap. Converted to `f32` only at spawn.
pub fn zombie_health(round: u32) -> u64 {
    let mut health = ZOMBIE_HEALTH_START;
    for r in 2..=round {
        health = if r >= 10 {
            health.saturating_add((health as f64 * ZOMBIE_HEALTH_GROWTH) as u64)
        } else {
            health.saturating_add(ZOMBIE_HEALTH_PER_ROUND)
        };
    }
    health
}

/// How many Zombies make up a Round's solo Horde:
/// `24 + int(3 × max(1, r/5) × (r ≥ 10 ? r × 0.15 : 1))`, scaled down in Rounds 1–5.
pub fn horde_size(round: u32) -> u32 {
    let r = round.max(1) as f64;
    let mut multiplier = (r / 5.0).max(1.0);
    if round >= 10 {
        multiplier *= r * 0.15;
    }
    // Float-to-int casts saturate, so huge Rounds pin at u32::MAX instead of wrapping.
    let size = HORDE_BASE.saturating_add((HORDE_SOLO_BONUS * multiplier) as u32);
    match HORDE_EARLY_SCALE.get(round.max(1) as usize - 1) {
        Some(scale) => (size as f64 * scale) as u32,
        None => size,
    }
}

/// Seconds between Zombie spawns in a Round: 2.0 s, ×0.95 each Round, never below the floor.
pub fn spawn_delay(round: u32) -> f32 {
    let shrink = SPAWN_DELAY_FACTOR.powf(round.saturating_sub(1) as f32);
    (SPAWN_DELAY_START_SECS * shrink).max(SPAWN_DELAY_MIN_SECS)
}

/// The range a new Zombie's speed roll is drawn from, uniformly: `[base, base + 34]`,
/// with base 1 in Round 1 and `(round − 1) × 8` after.
pub fn speed_roll_range(round: u32) -> RangeInclusive<u32> {
    let base = if round <= 1 {
        1
    } else {
        (round - 1).saturating_mul(SPEED_ROLL_PER_ROUND)
    };
    base..=base.saturating_add(SPEED_ROLL_SPREAD)
}

/// The Speed Tier a speed roll gives.
pub fn speed_tier(roll: u32) -> SpeedTier {
    if roll <= SPEED_ROLL_WALKER_MAX {
        SpeedTier::Walker
    } else if roll <= SPEED_ROLL_JOGGER_MAX {
        SpeedTier::Jogger
    } else {
        SpeedTier::Sprinter
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RoundPhase {
    Active,
    /// Seconds left of the Intermission.
    Intermission(f32),
}

#[derive(Resource)]
pub struct Round {
    pub number: u32,
    pub phase: RoundPhase,
    /// Zombies in this Round's Horde that haven't spawned yet.
    left_to_spawn: u32,
    spawn_timer: f32,
}

impl Round {
    fn begin(number: u32) -> Self {
        Self {
            number,
            phase: RoundPhase::Active,
            left_to_spawn: horde_size(number),
            spawn_timer: 0.0,
        }
    }
}

#[derive(Message)]
pub struct RoundStarted;

pub struct RoundPlugin;

impl Plugin for RoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<RoundStarted>()
            .add_systems(OnEnter(GameState::Playing), start_first_round)
            .add_systems(
                Update,
                (spawn_horde, last_zombie_sprints, end_round, run_intermission)
                    .chain()
                    .run_if(in_state(Pause::Running)),
            );
    }
}

fn start_first_round(mut commands: Commands, mut started: MessageWriter<RoundStarted>) {
    commands.insert_resource(Round::begin(1));
    started.write(RoundStarted);
}

fn spawn_horde(
    mut commands: Commands,
    mut round: ResMut<Round>,
    alive: Query<(), (With<Zombie>, Without<Dying>)>,
    spawn_points: Query<(&Transform, &SpawnPoint)>,
    active: Res<ActiveZones>,
    assets: ZombieAssetsParam,
    time: Res<Time>,
) {
    if round.phase != RoundPhase::Active || round.left_to_spawn == 0 {
        return;
    }
    round.spawn_timer -= time.delta_secs();
    if round.spawn_timer > 0.0 || alive.iter().count() as u32 >= ALIVE_CAP {
        return;
    }

    // Only Spawn Points in Active Zones send in Zombies.
    let points: Vec<_> = spawn_points
        .iter()
        .filter(|(_, point)| active.0.contains(point.0))
        .map(|(transform, _)| transform)
        .collect();
    if points.is_empty() {
        return;
    }
    let point = points[rand::random_range(0..points.len())];
    let jitter = Vec3::new(rand::random_range(-1.0..1.0), 0.0, rand::random_range(-1.0..1.0));
    let tier = speed_tier(rand::random_range(speed_roll_range(round.number)));

    spawn_zombie(
        &mut commands,
        &assets,
        point.translation + jitter,
        zombie_health(round.number) as f32,
        tier,
    );
    round.left_to_spawn -= 1;
    round.spawn_timer = spawn_delay(round.number);
}

/// In the Rounds listed, the Horde's last Zombie sprints so the Round can't be stalled.
fn last_zombie_sprints(round: Res<Round>, mut alive: Query<&mut Zombie, Without<Dying>>) {
    if round.phase != RoundPhase::Active
        || round.left_to_spawn > 0
        || !LAST_ZOMBIE_SPRINT_ROUNDS.contains(&round.number)
    {
        return;
    }
    if let Ok(mut last) = alive.single_mut() {
        last.sprint();
    }
}

fn end_round(mut round: ResMut<Round>, alive: Query<(), (With<Zombie>, Without<Dying>)>) {
    if round.phase == RoundPhase::Active && round.left_to_spawn == 0 && alive.is_empty() {
        round.phase = RoundPhase::Intermission(INTERMISSION_SECS);
    }
}

fn run_intermission(
    mut round: ResMut<Round>,
    mut started: MessageWriter<RoundStarted>,
    time: Res<Time>,
) {
    let RoundPhase::Intermission(left) = round.phase else {
        return;
    };
    let left = left - time.delta_secs();
    if left <= 0.0 {
        *round = Round::begin(round.number.saturating_add(1));
        started.write(RoundStarted);
    } else {
        round.phase = RoundPhase::Intermission(left);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_follows_the_bo1_table() {
        let table = [
            (1, 150),
            (2, 250),
            (3, 350),
            (5, 550),
            (9, 950),
            (10, 1045),
            (11, 1149),
            (12, 1263),
            (15, 1679),
            (20, 2701),
            (25, 4348),
            (30, 7000),
            (40, 18151),
            (50, 47073),
        ];
        for (round, health) in table {
            assert_eq!(zombie_health(round), health, "round {round}");
        }
    }

    #[test]
    fn health_has_no_cap_and_never_wraps() {
        let mut last = 0;
        for round in 1..=1000 {
            let health = zombie_health(round);
            assert!(health >= last, "round {round}");
            last = health;
        }
        assert_eq!(zombie_health(1000), u64::MAX);
    }

    #[test]
    fn horde_follows_the_bo1_solo_table() {
        let table = [
            (1, 6),
            (2, 8),
            (3, 13),
            (4, 18),
            (5, 24),
            (6, 27),
            (7, 28),
            (8, 28),
            (9, 29),
            (10, 33),
            (12, 36),
            (15, 44),
            (20, 60),
            (25, 80),
            (30, 105),
            (50, 249),
        ];
        for (round, size) in table {
            assert_eq!(horde_size(round), size, "round {round}");
        }
    }

    #[test]
    fn horde_never_shrinks_or_wraps() {
        for r in 1..10_000 {
            assert!(horde_size(r + 1) >= horde_size(r), "round {r}");
        }
        assert_eq!(horde_size(u32::MAX), u32::MAX);
    }

    #[test]
    fn horde_outgrows_alive_cap() {
        assert!(horde_size(6) > ALIVE_CAP);
    }

    #[test]
    fn spawn_delay_starts_at_two_seconds_and_shrinks_five_percent_a_round() {
        let table = [
            (1, 2.0),
            (5, 1.63),
            (10, 1.26),
            (15, 0.98),
            (20, 0.755),
            (30, 0.45),
            (50, 0.16),
        ];
        for (round, delay) in table {
            assert!((spawn_delay(round) - delay).abs() < 0.005, "round {round}: {}", spawn_delay(round));
        }
    }

    #[test]
    fn spawn_delay_bottoms_out_at_the_floor() {
        assert_eq!(spawn_delay(100), 0.08);
        assert_eq!(spawn_delay(u32::MAX), 0.08);
    }

    /// Every Speed Tier a Round's roll can produce.
    fn tiers_in(round: u32) -> Vec<SpeedTier> {
        let mut tiers: Vec<_> = speed_roll_range(round).map(speed_tier).collect();
        tiers.dedup();
        tiers
    }

    #[test]
    fn round_one_is_all_walkers() {
        assert_eq!(speed_roll_range(1), 1..=35);
        assert_eq!(tiers_in(1), [SpeedTier::Walker]);
    }

    #[test]
    fn joggers_join_from_round_two() {
        assert_eq!(speed_roll_range(2), 8..=42);
        assert_eq!(tiers_in(2), [SpeedTier::Walker, SpeedTier::Jogger]);
    }

    #[test]
    fn first_sprinters_appear_in_round_six() {
        assert_eq!(tiers_in(5), [SpeedTier::Walker, SpeedTier::Jogger]);
        assert_eq!(speed_roll_range(6), 40..=74);
        assert_eq!(tiers_in(6), [SpeedTier::Jogger, SpeedTier::Sprinter]);
    }

    #[test]
    fn every_zombie_sprints_from_round_ten() {
        assert_eq!(tiers_in(9), [SpeedTier::Jogger, SpeedTier::Sprinter]);
        for round in [10, 30, 100] {
            assert_eq!(tiers_in(round), [SpeedTier::Sprinter], "round {round}");
        }
    }

    #[test]
    fn speed_roll_never_wraps() {
        assert_eq!(*speed_roll_range(u32::MAX).end(), u32::MAX);
    }
}
