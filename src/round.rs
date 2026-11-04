use std::ops::RangeInclusive;

use bevy::prelude::*;

use crate::authority::HostSimulation;
use crate::barrier::Entering;
use crate::hellhound::{
    HellhoundAssetsParam, HellhoundSchedule, hellhound_health, hellhound_horde, hellhound_spawn_candidates,
    hellhound_spawn_gap, mixed_hound_chance, spawn_hellhound,
};
use crate::map::{SpawnKind, SpawnPoint};
use crate::modifier::{Modifier, Modifiers, Stat, resolve, resolve_count};
use crate::nova::{NovaAssetsParam, nova_crawler_entry, nova_crawler_health, spawn_nova_crawler, spawns_nova_crawler};
use crate::player::Player;
use crate::power::Power;
use crate::power_up::{DropPowerUp, PowerUp};
use crate::settings::Settings;
use crate::state::{GameState, Pause};
use crate::tuning::*;
use crate::zombie::{Dying, SpeedTier, Zombie, ZombieAssetsParam, ZombieCleanedUp, ZombieKilled, spawn_zombie};
use crate::zone::{ActiveZones, ZoneId, ZoneSet};

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

/// How many Zombies may be alive at once in a Round: 24 through Round 10, then 2 more each Round,
/// to 48 from Round 22. A Mode's `floor` holds the cap up until the ramp passes it.
pub fn alive_cap(round: u32, floor: u32) -> u32 {
    let grown = ALIVE_CAP
        .saturating_add(round.saturating_sub(ALIVE_CAP_GROWTH_FROM_ROUND).saturating_mul(ALIVE_CAP_PER_ROUND))
        .min(ALIVE_CAP_MAX);
    grown.max(floor)
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

/// A new Zombie's Speed Tier after the Player's stat entries: a Sprinter, whatever the Round
/// rolled, on a `roll` in `[0, 1)` under their `Stat::SprinterChance`.
pub fn spawn_tier(rolled: SpeedTier, player: &[Modifier], roll: f64) -> SpeedTier {
    if roll < f64::from(resolve(Stat::SprinterChance, 0.0, player)) {
        SpeedTier::Sprinter
    } else {
        rolled
    }
}

/// The Spawn Points a new Zombie picks from, uniformly: every one in an Active Zone, with no
/// line-of-sight exclusion. Hellhound Spawn Points are picked by `hellhound_spawn_candidates`.
pub fn spawn_candidates<T>(points: impl IntoIterator<Item = (ZoneId, T)>, active: ZoneSet) -> Vec<T> {
    points
        .into_iter()
        .filter(|(zone, _)| active.contains(*zone))
        .map(|(_, point)| point)
        .collect()
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
    /// Which Hellhound Round this is (1 for the first), if it is one. Its Horde is all hounds.
    pub hellhound: Option<u32>,
    /// How many Zombies make up this Round's Horde.
    horde: u32,
    /// Zombies in this Round's Horde that haven't spawned yet.
    left_to_spawn: u32,
    spawn_timer: f32,
    schedule: HellhoundSchedule,
    /// Where the Round's latest kill fell: in a Hellhound Round, the last hound's Max Ammo drops there.
    last_kill: Option<Vec3>,
}

impl Round {
    /// Round `number` begins, Hellhound Round or not by the Run's `schedule` (never with
    /// Hellhounds switched off). Its Horde follows the Player's stat entries.
    fn begin(number: u32, mut schedule: HellhoundSchedule, player: &[Modifier], hounds_on: bool) -> Self {
        let hellhound = schedule.begin(number, rand::random_range(HELLHOUND_ROUND_GAPS), hounds_on);
        let (horde, spawn_timer) = match hellhound {
            Some(count) => (hellhound_horde(count), HELLHOUND_FIRST_SPAWN_SECS),
            None => (horde_size(number), 0.0),
        };
        let left_to_spawn = resolve_count(Stat::HordeSize, horde, player);
        Self {
            number,
            phase: RoundPhase::Active,
            hellhound,
            horde: left_to_spawn,
            left_to_spawn,
            spawn_timer,
            schedule,
            last_kill: None,
        }
    }

    /// The Hellhound Round count whose health hounds outside a Hellhound Round use: the latest
    /// one so far.
    pub fn hellhound_tier(&self) -> u32 {
        self.schedule.played()
    }

    /// How many Zombies make up this Round's Horde.
    pub fn horde(&self) -> u32 {
        self.horde
    }

    /// How many of the Horde have spawned so far.
    pub fn horde_spawned(&self) -> u32 {
        self.horde.saturating_sub(self.left_to_spawn)
    }

    /// Takes one Zombie from the Horde's left-to-spawn count for something else to spawn in its
    /// place (a special). False when the Horde has all spawned.
    pub fn take_from_horde(&mut self) -> bool {
        if self.left_to_spawn == 0 {
            return false;
        }
        self.left_to_spawn -= 1;
        true
    }
}

/// A Zombie the Round doesn't wait for: if it's alive when the rest of the Horde is dead, the
/// Round ends anyway and it carries over into the next one. The special slot's Zombies have it.
#[derive(Component)]
pub struct CarriesOver;

#[derive(Message)]
pub struct RoundStarted;

/// The Round's last Zombie died and the Intermission began.
#[derive(Message)]
pub struct RoundEnded;

/// Held by anything layered on the Round that needs the Intermission to wait before counting
/// down. While held, the Intermission stays at its full length.
#[derive(Resource, Default)]
pub struct IntermissionHold(pub bool);

pub struct RoundPlugin;

impl Plugin for RoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<RoundStarted>()
            .add_message::<RoundEnded>()
            .init_resource::<IntermissionHold>()
            .add_systems(OnEnter(GameState::Playing), start_first_round.in_set(HostSimulation))
            .add_systems(
                Update,
                (requeue_cleaned_up, spawn_horde, last_zombie_sprints, end_round, run_intermission)
                    .chain()
                    .in_set(HostSimulation)
                    .run_if(in_state(Pause::Running)),
            );
    }
}

fn start_first_round(mut commands: Commands, settings: Res<Settings>, mut started: MessageWriter<RoundStarted>) {
    let schedule = HellhoundSchedule::new(rand::random_range(HELLHOUND_FIRST_ROUNDS));
    commands.insert_resource(Round::begin(1, schedule, &[], settings.special_zombies.hellhounds));
    commands.insert_resource(IntermissionHold::default());
    started.write(RoundStarted);
}

/// A cleaned-up Zombie goes back into the Horde's left-to-spawn count; its replacement rolls a
/// fresh Speed Tier when it spawns.
fn requeue_cleaned_up(mut round: ResMut<Round>, mut cleaned: MessageReader<ZombieCleanedUp>) {
    let count = cleaned.read().count() as u32;
    if count > 0 {
        round.left_to_spawn = round.left_to_spawn.saturating_add(count);
    }
}

/// Spawns the Horde: normal Zombies, a few of them hounds from Round 16 and Nova Crawlers after
/// Power, or all hounds in a Hellhound Round. Each special type comes only while switched on in
/// Settings, checked spawn by spawn, so a change mid-Run affects future spawns only.
fn spawn_horde(
    mut commands: Commands,
    mut round: ResMut<Round>,
    alive: Query<&Zombie, Without<Dying>>,
    spawn_points: Query<(&Transform, &SpawnPoint)>,
    player: Single<(&Transform, &Modifiers), With<Player>>,
    active: Res<ActiveZones>,
    assets: ZombieAssetsParam,
    hound_assets: HellhoundAssetsParam,
    nova_assets: NovaAssetsParam,
    power: Res<Power>,
    settings: Res<Settings>,
    time: Res<Time>,
) {
    if round.phase != RoundPhase::Active || round.left_to_spawn == 0 {
        return;
    }
    round.spawn_timer -= time.delta_secs();
    let (player, modifiers) = *player;
    let cap = if round.hellhound.is_some() {
        HELLHOUND_ALIVE_CAP
    } else {
        resolve_count(Stat::AliveCap, alive_cap(round.number, ALIVE_CAP), &modifiers.0)
    };
    if round.spawn_timer > 0.0 || alive.iter().count() as u32 >= cap {
        return;
    }

    let hound_points = || {
        let points = spawn_points
            .iter()
            .filter(|(_, point)| point.kind == SpawnKind::Hellhound)
            .map(|(t, point)| (point.zone, t.translation));
        hellhound_spawn_candidates(points, active.0, player.translation)
    };
    let on = settings.special_zombies;
    if let Some(count) = round.hellhound {
        // Hellhounds switched off mid-Round: no more come, and the Round ends with those alive.
        if !on.hellhounds {
            round.left_to_spawn = 0;
            return;
        }
        let points = hound_points();
        if points.is_empty() {
            return;
        }
        spawn_hellhound(&mut commands, &hound_assets, points[rand::random_range(0..points.len())], hellhound_health(count));
        round.left_to_spawn -= 1;
        round.spawn_timer = hellhound_spawn_gap(count);
        return;
    }

    // Mixed hounds: a few normal spawns are hounds instead, at the latest tier's health.
    let hounds_alive = alive.iter().filter(|zombie| zombie.is_hellhound()).count() as u32;
    if hounds_alive < HELLHOUND_MIXED_MAX_ALIVE && rand::random_bool(mixed_hound_chance(round.number, on.hellhounds)) {
        let points = hound_points();
        if !points.is_empty() {
            let health = hellhound_health(round.hellhound_tier());
            spawn_hellhound(&mut commands, &hound_assets, points[rand::random_range(0..points.len())], health);
            round.left_to_spawn -= 1;
            round.spawn_timer = spawn_delay(round.number);
            return;
        }
    }

    // Nova Crawlers: after Power, a few normal spawns drop from Vents (or rise in the Yard) instead.
    let novas_alive = alive.iter().filter(|zombie| zombie.is_nova_crawler()).count() as u32;
    if spawns_nova_crawler(on.nova_crawlers, power.is_on(), round.hellhound.is_some(), novas_alive, rand::random()) {
        let points = spawn_candidates(
            spawn_points
                .iter()
                .filter(|(_, point)| nova_crawler_entry(point.kind))
                .map(|(t, point)| (point.zone, (t.translation, point.kind))),
            active.0,
        );
        if !points.is_empty() {
            let (at, kind) = points[rand::random_range(0..points.len())];
            spawn_nova_crawler(&mut commands, &nova_assets, kind, at, nova_crawler_health(round.number));
            round.left_to_spawn -= 1;
            round.spawn_timer = spawn_delay(round.number);
            return;
        }
    }

    let points = spawn_candidates(
        spawn_points
            .iter()
            .filter(|(_, point)| matches!(point.kind, SpawnKind::Window(_) | SpawnKind::Riser))
            .map(|(t, point)| (point.zone, (t, point.kind))),
        active.0,
    );
    if points.is_empty() {
        return;
    }
    let (point, kind) = points[rand::random_range(0..points.len())];
    let jitter = Vec3::new(rand::random_range(-0.75..0.75), 0.0, rand::random_range(-0.75..0.75));
    let tier = speed_tier(rand::random_range(speed_roll_range(round.number)));
    let tier = spawn_tier(tier, &modifiers.0, rand::random());

    let zombie = spawn_zombie(
        &mut commands,
        &assets,
        point.translation + jitter,
        zombie_health(round.number) as f32,
        tier,
        (kind == SpawnKind::Riser).then_some(ZOMBIE_RISE_SECS),
    );
    // A Zombie outside a window has to get in through its Barrier.
    if let SpawnKind::Window(barrier) = kind {
        commands.entity(zombie).insert(Entering::new(barrier));
    }
    round.left_to_spawn -= 1;
    round.spawn_timer = spawn_delay(round.number);
}

/// In the Rounds listed, the Horde's last Zombie sprints so the Round can't be stalled.
fn last_zombie_sprints(round: Res<Round>, mut alive: Query<&mut Zombie, (Without<Dying>, Without<CarriesOver>)>) {
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

/// The Round ends with its last Zombie, not counting any that carry over. In a Hellhound Round,
/// the last hound drops a Max Ammo, outside the per-Round cap.
fn end_round(
    mut round: ResMut<Round>,
    alive: Query<(), (With<Zombie>, Without<Dying>, Without<CarriesOver>)>,
    mut kills: MessageReader<ZombieKilled>,
    mut ended: MessageWriter<RoundEnded>,
    mut drops: MessageWriter<DropPowerUp>,
) {
    if let Some(kill) = kills.read().last() {
        round.last_kill = Some(kill.at);
    }
    if round.phase == RoundPhase::Active && round.left_to_spawn == 0 && alive.is_empty() {
        round.phase = RoundPhase::Intermission(INTERMISSION_SECS);
        ended.write(RoundEnded);
        if round.hellhound.is_some()
            && let Some(at) = round.last_kill
        {
            drops.write(DropPowerUp {
                at,
                power_up: Some(PowerUp::MaxAmmo),
            });
        }
    }
}

fn run_intermission(
    mut round: ResMut<Round>,
    hold: Res<IntermissionHold>,
    player: Query<&Modifiers, With<Player>>,
    settings: Res<Settings>,
    mut started: MessageWriter<RoundStarted>,
    time: Res<Time>,
) {
    let RoundPhase::Intermission(left) = round.phase else {
        return;
    };
    if hold.0 {
        round.phase = RoundPhase::Intermission(INTERMISSION_SECS);
        return;
    }
    let left = left - time.delta_secs();
    if left <= 0.0 {
        let entries: Vec<Modifier> = player.iter().flat_map(|modifiers| modifiers.0.iter().copied()).collect();
        *round = Round::begin(
            round.number.saturating_add(1),
            round.schedule,
            &entries,
            settings.special_zombies.hellhounds,
        );
        started.write(RoundStarted);
    } else {
        round.phase = RoundPhase::Intermission(left);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zombies_spawn_only_in_active_zones() {
        use ZoneId::*;
        let points = [(StartRoom, 0), (StartRoom, 1), (WestHall, 2), (Yard, 3), (Balcony, 4)];
        let active = ZoneSet::from_iter([StartRoom, Yard]);
        assert_eq!(spawn_candidates(points, active), vec![0, 1, 3]);
        assert!(spawn_candidates(points, ZoneSet::from_iter([DiningRoom])).is_empty());
    }

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
    fn alive_cap_follows_the_ramp() {
        let table = [
            (1, 24),
            (10, 24),
            (11, 26),
            (15, 34),
            (21, 46),
            (22, 48),
            (23, 48),
            (50, 48),
            (u32::MAX, 48),
        ];
        for (round, cap) in table {
            assert_eq!(alive_cap(round, ALIVE_CAP), cap, "round {round}");
        }
    }

    #[test]
    fn a_floor_holds_the_alive_cap_up_until_the_ramp_passes_it() {
        assert_eq!(alive_cap(1, 32), 32);
        assert_eq!(alive_cap(14, 32), 32);
        assert_eq!(alive_cap(15, 32), 34);
        assert_eq!(alive_cap(30, 32), 48);
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
    fn a_sprinter_chance_entry_makes_sprinters_from_round_one() {
        use crate::modifier::{Effect, Source};
        let always = [Modifier { stat: Stat::SprinterChance, source: Source("A"), effect: Effect::Chance(1.0, 1) }];
        for roll in [0.0, 0.5, 0.999] {
            assert_eq!(spawn_tier(SpeedTier::Walker, &always, roll), SpeedTier::Sprinter);
            assert_eq!(spawn_tier(SpeedTier::Walker, &[], roll), SpeedTier::Walker, "Classic keeps the roll");
        }
    }

    #[test]
    fn horde_and_alive_cap_entries_resize_them() {
        use crate::modifier::{Effect, Source};
        let bigger = [Modifier { stat: Stat::HordeSize, source: Source("A"), effect: Effect::Mult(1.25) }];
        assert_eq!(resolve_count(Stat::HordeSize, horde_size(10), &bigger), (horde_size(10) as f32 * 1.25).ceil() as u32);
        let cap = [Modifier { stat: Stat::AliveCap, source: Source("A"), effect: Effect::Flat(8.0) }];
        assert_eq!(resolve_count(Stat::AliveCap, ALIVE_CAP, &cap), 32);
    }

    #[test]
    fn speed_roll_never_wraps() {
        assert_eq!(*speed_roll_range(u32::MAX).end(), u32::MAX);
    }
}
