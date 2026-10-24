use bevy::prelude::*;

use crate::map::SpawnPoint;
use crate::state::{GameState, Pause};
use crate::tuning::*;
use crate::zombie::{Dying, SpeedTier, Zombie, ZombieAssetsParam, spawn_zombie};

/// Zombie health for a Round: +100 per Round up to Round 9, then +10% per Round.
pub fn zombie_health(round: u32) -> f32 {
    let round = round.max(1);
    if round < 10 {
        150.0 + 100.0 * (round - 1) as f32
    } else {
        950.0 * 1.1_f32.powi((round - 9) as i32)
    }
}

/// How many Zombies make up a Round's Horde. A simplified version of the solo curve.
pub fn horde_size(round: u32) -> u32 {
    match round.max(1) {
        r @ 1..=4 => [6, 8, 13, 18][(r - 1) as usize],
        r => {
            let r = r as f32;
            (0.09 * r * r + 0.7 * r + 20.0) as u32
        }
    }
}

/// Picks a Speed Tier for a new Zombie. `roll` is uniform in `[0, 1)`.
/// Round 1 is all Walkers; Joggers take over from Round 2 and Sprinters appear from Round 4.
pub fn speed_tier(round: u32, roll: f32) -> SpeedTier {
    let r = round.max(1) as f32;
    let sprint = (0.15 * (r - 3.0)).clamp(0.0, 0.85);
    let jog = (0.25 * (r - 1.0)).clamp(0.0, 1.0) * (1.0 - sprint);
    if roll < sprint {
        SpeedTier::Sprinter
    } else if roll < sprint + jog {
        SpeedTier::Jogger
    } else {
        SpeedTier::Walker
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
                (spawn_horde, end_round, run_intermission)
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
    spawn_points: Query<&Transform, With<SpawnPoint>>,
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

    let points: Vec<_> = spawn_points.iter().collect();
    if points.is_empty() {
        return;
    }
    let point = points[rand::random_range(0..points.len())];
    let jitter = Vec3::new(rand::random_range(-1.0..1.0), 0.0, rand::random_range(-1.0..1.0));
    let tier = speed_tier(round.number, rand::random());

    spawn_zombie(
        &mut commands,
        &assets,
        point.translation + jitter,
        zombie_health(round.number),
        tier,
    );
    round.left_to_spawn -= 1;
    round.spawn_timer = SPAWN_INTERVAL_SECS;
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
        *round = Round::begin(round.number + 1);
        started.write(RoundStarted);
    } else {
        round.phase = RoundPhase::Intermission(left);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_grows_linearly_then_exponentially() {
        assert_eq!(zombie_health(1), 150.0);
        assert_eq!(zombie_health(2), 250.0);
        assert_eq!(zombie_health(9), 950.0);
        assert!((zombie_health(10) - 1045.0).abs() < 0.01);
        assert!(zombie_health(20) > zombie_health(19) * 1.09);
    }

    #[test]
    fn horde_starts_small_and_keeps_growing() {
        assert_eq!(horde_size(1), 6);
        assert_eq!(horde_size(4), 18);
        for r in 1..50 {
            assert!(horde_size(r + 1) > horde_size(r), "round {r}");
        }
    }

    #[test]
    fn horde_outgrows_alive_cap() {
        assert!(horde_size(6) > ALIVE_CAP);
    }

    #[test]
    fn round_one_is_all_walkers() {
        for roll in [0.0, 0.5, 0.999] {
            assert_eq!(speed_tier(1, roll), SpeedTier::Walker);
        }
    }

    #[test]
    fn sprinters_appear_later_but_never_take_over_entirely() {
        assert_ne!(speed_tier(3, 0.0), SpeedTier::Sprinter);
        assert_eq!(speed_tier(4, 0.0), SpeedTier::Sprinter);
        assert_ne!(speed_tier(100, 0.99), SpeedTier::Sprinter);
    }
}
