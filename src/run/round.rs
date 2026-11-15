use std::ops::RangeInclusive;

use bevy::prelude::*;

use crate::authority::HostSimulation;
use crate::maps::barrier::Entering;
use crate::enemies::hellhound::{
    HellhoundAssetsParam, HellhoundSchedule, hellhound_alive_cap, hellhound_health, hellhound_horde, hellhound_spawn_candidates,
    hellhound_spawn_gap, mixed_hound_chance, spawn_hellhound,
};
use crate::maps::map::{Layout, MapId, SpawnKind, SpawnPoint};
use crate::maps::water::{TideLevel, wading_point_active};
use crate::modifier::{Modifier, Modifiers, Stat, resolve, resolve_count};
use crate::enemies::nova::{NovaAssetsParam, nova_crawler_entry, nova_crawler_health, spawn_nova_crawler, spawns_nova_crawler};
use crate::players::player::{LocalPlayer, Player, SpawnPlayers};
use crate::maps::power::Power;
use crate::economy::power_up::{DropPowerUp, PowerUp};
use crate::storage::settings::Settings;
use crate::state::{GameState, world_runs};
use crate::enemies::target::Players;
use crate::tuning::*;
use crate::enemies::zombie::{Dying, SpeedTier, Zombie, ZombieAssetsParam, ZombieCleanedUp, ZombieKilled, spawn_zombie};
use crate::maps::zone::{ActiveZones, ZoneId, ZoneSet};

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

/// How many Zombies make up a Round's solo Horde: `horde_size_for(round, 1)`.
#[cfg(test)]
pub fn horde_size(round: u32) -> u32 {
    horde_size_for(round, 1)
}

/// How many Zombies make up a Round's Horde with `players` Players (N), where
/// `m = max(1, r/5) × (r ≥ 10 ? r × 0.15 : 1)`: `24 + int(3m)` solo, `24 + int((N − 1) × 6 × m)`
/// in Co-op, scaled down in Rounds 1–5.
pub fn horde_size_for(round: u32, players: u32) -> u32 {
    let r = round.max(1) as f64;
    let mut multiplier = (r / 5.0).max(1.0);
    if round >= 10 {
        multiplier *= r * 0.15;
    }
    let bonus = if players <= 1 { HORDE_SOLO_BONUS } else { f64::from(players - 1) * HORDE_COOP_PER_PLAYER };
    // Float-to-int casts saturate, so huge Rounds pin at u32::MAX instead of wrapping.
    let size = HORDE_BASE.saturating_add((bonus * multiplier) as u32);
    match HORDE_EARLY_SCALE.get(round.max(1) as usize - 1) {
        Some(scale) => (size as f64 * scale) as u32,
        None => size,
    }
}

/// How many Zombies may be alive at once in a Round: 24 through Round 10, then 2 more each Round,
/// to 48 from Round 22. A Mode's (or a Co-op Run's) `floor` holds the cap up until the ramp passes it.
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

/// Of `points` (each with where it stands), those within `ZOMBIE_SPAWN_MAX_DIST` of one of
/// `players`: a Zombie from farther off would count as lost and be cleaned up before it got
/// there, which on a long Map is most of an Active Zone. Every one of them if none is that near.
pub fn near_enough<T>(points: Vec<(Vec3, T)>, players: &[Vec3]) -> Vec<T> {
    let near = |at: &Vec3| players.iter().any(|player| at.distance(*player) <= ZOMBIE_SPAWN_MAX_DIST);
    let any_near = points.iter().any(|(at, _)| near(at));
    points.into_iter().filter(|(at, _)| !any_near || near(at)).map(|(_, point)| point).collect()
}

/// Round numbers kept clear of Classic's own schedulers: a Hellhound Round, a Sapper or a special
/// slot's Napalm or Shrieker that would land on one slides to the next Round that isn't reserved.
/// Empty in Classic; anything layered on it may reserve Rounds for its own use (ADR 0007).
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct ReservedRounds(pub std::collections::BTreeSet<u32>);

impl ReservedRounds {
    pub fn contains(&self, round: u32) -> bool {
        self.0.contains(&round)
    }
}

/// Whether Round `round` is a Hellhound Round by the Run's `schedule`, and which (1 for the
/// first): never a reserved one, which leaves the schedule as it was so the Hellhound Round slides
/// to the next Round that isn't reserved.
pub fn hellhound_round(
    schedule: &mut HellhoundSchedule,
    round: u32,
    gap: u32,
    hounds_on: bool,
    reserved: &ReservedRounds,
) -> Option<u32> {
    if reserved.contains(round) {
        return None;
    }
    schedule.begin(round, gap, hounds_on)
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
    /// The Players connected when the Round began (N). Fixed for the Round.
    players: u32,
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
    /// `players` is N, the Players connected now.
    #[cfg(test)]
    fn begin(number: u32, schedule: HellhoundSchedule, player: &[Modifier], hounds_on: bool, players: u32) -> Self {
        Self::begin_in(number, schedule, player, hounds_on, players, &ReservedRounds::default())
    }

    /// `begin`, with `reserved` Rounds kept clear of Hellhound Rounds.
    fn begin_in(
        number: u32,
        mut schedule: HellhoundSchedule,
        player: &[Modifier],
        hounds_on: bool,
        players: u32,
        reserved: &ReservedRounds,
    ) -> Self {
        let gap = rand::random_range(HELLHOUND_ROUND_GAPS);
        let hellhound = hellhound_round(&mut schedule, number, gap, hounds_on, reserved);
        let (horde, spawn_timer) = match hellhound {
            Some(count) => (hellhound_horde(count, players), HELLHOUND_FIRST_SPAWN_SECS),
            None => (horde_size_for(number, players), 0.0),
        };
        let left_to_spawn = resolve_count(Stat::HordeSize, horde, player);
        Self {
            number,
            phase: RoundPhase::Active,
            hellhound,
            players,
            horde: left_to_spawn,
            left_to_spawn,
            spawn_timer,
            schedule,
            last_kill: None,
        }
    }

    /// Round `number` as a client shows it, as the Host says: nothing of the Horde is known here.
    pub fn shown(number: u32, hellhound: Option<u32>) -> Self {
        Self {
            number,
            phase: RoundPhase::Active,
            hellhound,
            players: 1,
            horde: 0,
            left_to_spawn: 0,
            spawn_timer: 0.0,
            schedule: HellhoundSchedule::new(u32::MAX),
            last_kill: None,
        }
    }

    /// The Hellhound Round count whose health hounds outside a Hellhound Round use: the latest
    /// one so far.
    pub fn hellhound_tier(&self) -> u32 {
        self.schedule.played()
    }

    /// The Players connected when this Round began.
    #[allow(dead_code, reason = "later Co-op rules read N too")]
    pub fn players(&self) -> u32 {
        self.players
    }

    /// The most Zombies alive at once this Round, before any stat entries: the ramp, held up by
    /// `ALIVE_CAP_COOP` when more than one Player is in.
    pub fn alive_cap(&self) -> u32 {
        alive_cap(self.number, if self.players > 1 { ALIVE_CAP_COOP } else { ALIVE_CAP })
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
            .init_resource::<ReservedRounds>()
            .add_systems(
                OnEnter(GameState::Playing),
                start_first_round.in_set(HostSimulation).after(SpawnPlayers),
            )
            .add_systems(
                Update,
                (requeue_cleaned_up, spawn_horde, last_zombie_sprints, end_round, run_intermission)
                    .chain()
                    .in_set(HostSimulation)
                    .run_if(world_runs),
            );
    }
}

fn start_first_round(
    mut commands: Commands,
    settings: Res<Settings>,
    players: Query<(), With<Player>>,
    reserved: Res<ReservedRounds>,
    mut started: MessageWriter<RoundStarted>,
) {
    let schedule = HellhoundSchedule::new(rand::random_range(HELLHOUND_FIRST_ROUNDS));
    let hounds_on = settings.special_zombies.hellhounds;
    let players = players.iter().count() as u32;
    commands.insert_resource(Round::begin_in(1, schedule, &[], hounds_on, players, &reserved));
    commands.insert_resource(IntermissionHold::default());
    started.write(RoundStarted);
}

/// A cleaned-up Zombie goes back into the Horde's left-to-spawn count; its replacement rolls a
/// fresh Speed Tier when it spawns.
pub(crate) fn requeue_cleaned_up(mut round: ResMut<Round>, mut cleaned: MessageReader<ZombieCleanedUp>) {
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
    modifiers: Single<&Modifiers, With<LocalPlayer>>,
    players: Players,
    active: Res<ActiveZones>,
    map: Option<Res<MapId>>,
    assets: ZombieAssetsParam,
    hound_assets: HellhoundAssetsParam,
    nova_assets: NovaAssetsParam,
    power: Res<Power>,
    settings: Res<Settings>,
    tide: Option<Res<crate::maps::tide::Tide>>,
    time: Res<Time>,
) {
    if round.phase != RoundPhase::Active || round.left_to_spawn == 0 {
        return;
    }
    round.spawn_timer -= time.delta_secs();
    let cap = if round.hellhound.is_some() {
        hellhound_alive_cap(players.up_count())
    } else {
        resolve_count(Stat::AliveCap, round.alive_cap(), &modifiers.0)
    };
    if round.spawn_timer > 0.0 || alive.iter().count() as u32 >= cap {
        return;
    }

    // Where the Players new arrivals are placed round: those who are up, or all if none is.
    let anchors: Vec<Vec3> = players.spawn_anchors().iter().map(|spot| spot.centre).collect();
    let hound_points = || {
        let points = spawn_points
            .iter()
            .filter(|(_, point)| point.kind == SpawnKind::Hellhound)
            .map(|(t, point)| (point.zone, t.translation));
        hellhound_spawn_candidates(points, active.0, &anchors)
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
                .map(|(t, point)| (point.zone, (t.translation, (t.translation, point.kind)))),
            active.0,
        );
        let points = near_enough(points, &anchors);
        if !points.is_empty() {
            let (at, kind) = points[rand::random_range(0..points.len())];
            spawn_nova_crawler(&mut commands, &nova_assets, kind, at, nova_crawler_health(round.number));
            round.left_to_spawn -= 1;
            round.spawn_timer = spawn_delay(round.number);
            return;
        }
    }

    // Windows and Risers, and the Wading Points whose water is Shallow at this Tide.
    let points = horde_points(
        spawn_points.iter().map(|(t, point)| (point.zone, t, point.kind)),
        map.map(|map| map.layout()),
        tide.map_or_else(TideLevel::default, |tide| tide.level()),
        active.0,
        &anchors,
    );
    if points.is_empty() {
        return;
    }
    let (point, kind) = points[rand::random_range(0..points.len())];
    let zombie = spawn_normal_zombie(&mut commands, &assets, round.number, &modifiers.0, point.translation, kind);
    if kind == SpawnKind::Wading {
        face_nearest(&mut commands, zombie, point.translation, &anchors);
    }
    round.left_to_spawn -= 1;
    round.spawn_timer = spawn_delay(round.number);
}

/// Whether a Spawn Point of `kind` at `at` sends the Horde's Zombies: a Window or a Riser always,
/// and a Wading Point only while the water over it is Shallow at `tide`. Hellhounds, specials and
/// Vents have their own entries and are never the Horde's.
pub fn horde_entry(kind: SpawnKind, layout: Option<&Layout>, tide: TideLevel, at: Vec3) -> bool {
    match kind {
        SpawnKind::Window(_) | SpawnKind::Riser => true,
        SpawnKind::Wading => layout.is_some_and(|layout| wading_point_active(layout, tide, at)),
        SpawnKind::Hellhound | SpawnKind::Special | SpawnKind::Vent => false,
    }
}

/// Where the Horde's next Zombie can come in: of the Spawn Points `(zone, where, kind)`, the
/// Horde's entries (see `horde_entry`) in Active Zones that are near enough a Player.
pub fn horde_points<'a>(
    points: impl IntoIterator<Item = (ZoneId, &'a Transform, SpawnKind)>,
    layout: Option<&Layout>,
    tide: TideLevel,
    active: ZoneSet,
    players: &[Vec3],
) -> Vec<(&'a Transform, SpawnKind)> {
    let points = spawn_candidates(
        points
            .into_iter()
            .filter(|(_, t, kind)| horde_entry(*kind, layout, tide, t.translation))
            .map(|(zone, t, kind)| (zone, (t.translation, (t, kind)))),
        active,
    );
    near_enough(points, players)
}

/// Turns `zombie`, just spawned at `at`, to face the nearest of `players` (those who are up).
pub fn face_nearest(commands: &mut Commands, zombie: Entity, at: Vec3, players: &[Vec3]) {
    let Some(nearest) = players.iter().min_by(|a, b| a.distance_squared(at).total_cmp(&b.distance_squared(at))) else {
        return;
    };
    let to = Vec3::new(nearest.x - at.x, 0.0, nearest.z - at.z);
    if to.length_squared() > 1e-4 {
        let yaw = Quat::from_rotation_y(f32::atan2(-to.x, -to.z));
        commands.entity(zombie).entry::<Transform>().and_modify(move |mut transform| transform.rotation = yaw);
    }
}

/// How long a Zombie from a `kind` of Spawn Point takes to rise, if it rises.
pub fn rise_secs(kind: SpawnKind) -> Option<f32> {
    match kind {
        SpawnKind::Riser => Some(ZOMBIE_RISE_SECS),
        SpawnKind::Wading => Some(ZOMBIE_WADE_RISE_SECS),
        _ => None,
    }
}

/// A normal Zombie at Round `number`'s health, rolling its Speed Tier, at the spawn point `at` of
/// `kind`: a Riser or Wading Point's Zombie rises, and one outside a window has to get in through its Barrier.
pub fn spawn_normal_zombie(
    commands: &mut Commands,
    assets: &ZombieAssetsParam,
    number: u32,
    player: &[Modifier],
    at: Vec3,
    kind: SpawnKind,
) -> Entity {
    let jitter = Vec3::new(rand::random_range(-0.75..0.75), 0.0, rand::random_range(-0.75..0.75));
    let tier = speed_tier(rand::random_range(speed_roll_range(number)));
    let tier = spawn_tier(tier, player, rand::random());
    let zombie = spawn_zombie(
        commands,
        assets,
        at + jitter,
        zombie_health(number) as f32,
        tier,
        rise_secs(kind),
    );
    if let SpawnKind::Window(barrier) = kind {
        commands.entity(zombie).insert(Entering::new(barrier));
    }
    zombie
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
    reserved: Res<ReservedRounds>,
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
        *round = Round::begin_in(
            round.number.saturating_add(1),
            round.schedule,
            &entries,
            settings.special_zombies.hellhounds,
            player.iter().count() as u32,
            &reserved,
        );
        started.write(RoundStarted);
    } else {
        round.phase = RoundPhase::Intermission(left);
    }
}

#[cfg(test)]
impl Round {
    /// Round `number`, under way, for headless tests of rules that depend on the Round.
    pub fn in_progress(number: u32) -> Self {
        Self::begin(number, HellhoundSchedule::new(1000), &[], false, 2)
    }
}

impl Round {
    /// Round `number`, under way with `players` Players and no Hellhounds, for the benchmark to
    /// jump straight to a late Round.
    pub fn for_bench(number: u32, players: u32) -> Self {
        Self::begin_in(number, HellhoundSchedule::new(u32::MAX), &[], false, players, &ReservedRounds::default())
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

    /// On a long Map an Active Zone can reach far past the cleanup distance: a Zombie only comes
    /// from a Spawn Point near enough a Player to arrive, unless none is.
    #[test]
    fn a_zombie_comes_from_a_spawn_point_near_enough_a_player() {
        let points = vec![(Vec3::new(10.0, 0.0, 0.0), 'a'), (Vec3::new(80.0, 0.0, 0.0), 'b'), (Vec3::new(0.0, 0.0, -30.0), 'c')];
        assert_eq!(near_enough(points.clone(), &[Vec3::ZERO]), vec!['a', 'c']);
        // A second Player out by the far one brings it in reach too.
        assert_eq!(near_enough(points.clone(), &[Vec3::ZERO, Vec3::new(90.0, 0.0, 0.0)]), vec!['a', 'b', 'c']);
        // With none near, any will do rather than none.
        assert_eq!(near_enough(points, &[Vec3::new(0.0, 0.0, 200.0)]), vec!['a', 'b', 'c']);
    }

    /// The Cove's Spawn Points as the pass sees them: its Windows, and its eight Wading Points.
    fn cove_points() -> Vec<(ZoneId, Transform, SpawnKind)> {
        use crate::maps::map::{WINDOWS, window_spawn};
        let windows = MapId::Cove
            .barriers()
            .map(|barrier| (WINDOWS[barrier.0 as usize].0, Transform::from_translation(window_spawn(barrier)), SpawnKind::Window(barrier)));
        let wading = MapId::Cove.layout().wading.iter().map(|&(zone, at)| (zone, Transform::from_translation(at), SpawnKind::Wading));
        windows.chain(wading).collect()
    }

    /// Wading Points join Windows and Risers under the same Active Zones and near-enough rule: the
    /// Strand's four come in while it's Active, and the Sea Cave's one only while the Sea Cave is.
    #[test]
    fn the_spawn_pass_picks_wading_points_only_in_active_zones() {
        use ZoneId::*;
        let points = cove_points();
        let pick = |zones: &[ZoneId], players: &[Vec3]| {
            let active = ZoneSet::from_iter(zones.iter().copied());
            horde_points(points.iter().map(|(zone, t, kind)| (*zone, t, *kind)), Some(MapId::Cove.layout()), TideLevel::Low, active, players)
        };
        let wading = |picked: &[(&Transform, SpawnKind)]| picked.iter().filter(|(_, kind)| *kind == SpawnKind::Wading).count();
        // Players spread along the shore, so every point is near enough one of them.
        let middle: Vec<Vec3> = [-45.0, -20.0, 0.0, 27.0].iter().map(|x| Vec3::new(*x, 0.0, -4.0)).collect();
        assert_eq!(wading(&pick(&[Strand], &middle)), 4);
        assert_eq!(wading(&pick(&[Strand, SeaCave], &middle)), 5);
        assert_eq!(wading(&pick(&[Strand, Shingle, SeaCave, Boathouse], &middle)), 8);
        // Not in a Zone with none, nor a Zone that isn't Active.
        assert_eq!(wading(&pick(&[Taverna], &middle)), 0);
        assert!(pick(&[], &middle).is_empty());
        // Near enough: a Player at the far east end is out of reach of the western points.
        let east = [Vec3::new(30.0, 0.0, -8.0)];
        let near = pick(&[Strand, Shingle, SeaCave, Boathouse], &east);
        assert!(near.iter().all(|(t, _)| t.translation.distance(east[0]) <= ZOMBIE_SPAWN_MAX_DIST));
        assert!(wading(&near) > 0 && wading(&near) < 8);
    }

    /// Only the Horde's own entries send Zombies, a Wading Point only over Shallow Water; Nova
    /// Crawlers keep to Vents and Risers, so the Cove, with neither, has none.
    #[test]
    fn only_the_hordes_own_entries_send_zombies_and_the_cove_has_no_nova_crawlers() {
        let cove = MapId::Cove.layout();
        let at = cove.wading[0].1;
        let barrier = crate::maps::map::BarrierId(0);
        assert!(horde_entry(SpawnKind::Window(barrier), None, TideLevel::Low, Vec3::ZERO));
        assert!(horde_entry(SpawnKind::Riser, None, TideLevel::Low, Vec3::ZERO));
        assert!(horde_entry(SpawnKind::Wading, Some(cove), TideLevel::Low, at));
        assert!(!horde_entry(SpawnKind::Wading, Some(MapId::Greybox.layout()), TideLevel::Low, at), "no water over it");
        assert!(!horde_entry(SpawnKind::Wading, None, TideLevel::Low, at));
        for kind in [SpawnKind::Hellhound, SpawnKind::Special, SpawnKind::Vent] {
            assert!(!horde_entry(kind, Some(cove), TideLevel::Low, at));
        }
        assert!(!nova_crawler_entry(SpawnKind::Wading));
        assert!(cove_points().iter().all(|(_, _, kind)| !nova_crawler_entry(*kind)));
        assert!(cove.vents.is_empty() && cove.risers.is_empty());
        assert_eq!(rise_secs(SpawnKind::Wading), Some(ZOMBIE_WADE_RISE_SECS));
        assert_eq!(rise_secs(SpawnKind::Riser), Some(ZOMBIE_RISE_SECS));
        assert_eq!(rise_secs(SpawnKind::Window(barrier)), None);
        assert_eq!(ZOMBIE_WADE_RISE_SECS, 2.0);
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
    fn coop_horde_follows_the_player_count() {
        // (round, [N = 2, 3, 4])
        let table = [(10, [42, 60, 78]), (20, [96, 168, 240])];
        for (round, sizes) in table {
            assert_eq!([2, 3, 4].map(|n| horde_size_for(round, n)), sizes, "round {round}");
        }
        for round in [1, 5, 10, 20, 50] {
            assert_eq!(horde_size_for(round, 1), horde_size(round), "round {round}");
            assert_eq!(horde_size_for(round, 0), horde_size(round), "round {round}");
        }
        assert_eq!(horde_size(10), 24 + (3.0 * 3.0) as u32);
        assert_eq!(horde_size_for(u32::MAX, 4), u32::MAX);
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
    fn the_coop_alive_cap_is_at_least_32_and_follows_the_ramp_above_it() {
        let round = |number, players| Round::begin(number, HellhoundSchedule::new(99), &[], false, players);
        assert_eq!(round(1, 1).alive_cap(), 24);
        assert_eq!(round(1, 2).alive_cap(), 32);
        assert_eq!(round(14, 4).alive_cap(), 32);
        assert_eq!(round(15, 3).alive_cap(), 34);
        assert_eq!(round(30, 2).alive_cap(), 48);
        // Overkill's flat +8 goes on top: the ceiling is 56.
        let overkill = [Modifier { stat: Stat::AliveCap, source: crate::modifier::Source("A"), effect: crate::modifier::Effect::Flat(PACT_OVERKILL_ALIVE_CAP_BONUS as f32) }];
        assert_eq!(resolve_count(Stat::AliveCap, round(50, 4).alive_cap(), &overkill), 56);
    }

    #[test]
    fn a_round_keeps_the_player_count_it_began_with() {
        let round = Round::begin(10, HellhoundSchedule::new(99), &[], false, 3);
        assert_eq!((round.players(), round.horde()), (3, 60));
        let hounds = Round::begin(5, HellhoundSchedule::new(5), &[], true, 3);
        assert_eq!((hounds.hellhound, hounds.horde()), (Some(1), 18));
    }

    fn reserved(rounds: &[u32]) -> ReservedRounds {
        ReservedRounds(rounds.iter().copied().collect())
    }

    #[test]
    fn a_hellhound_round_on_a_reserved_round_slides_one_round_later() {
        let mut schedule = HellhoundSchedule::new(10);
        let reserved = reserved(&[10]);
        assert_eq!(hellhound_round(&mut schedule, 9, 4, true, &reserved), None);
        assert_eq!(hellhound_round(&mut schedule, 10, 4, true, &reserved), None, "Round 10 is reserved");
        assert_eq!(hellhound_round(&mut schedule, 11, 4, true, &reserved), Some(1));
        assert_eq!(hellhound_round(&mut schedule, 15, 4, true, &reserved), Some(2), "the next counts on from 11");
    }

    #[test]
    fn a_hellhound_round_slides_past_reserved_rounds_back_to_back() {
        let mut schedule = HellhoundSchedule::new(10);
        let reserved = reserved(&[10, 11]);
        let hound_rounds: Vec<u32> =
            (1..=12).filter(|&r| hellhound_round(&mut schedule, r, 4, true, &reserved).is_some()).collect();
        assert_eq!(hound_rounds, [12]);
    }

    #[test]
    fn with_nothing_reserved_hellhound_rounds_keep_their_schedule() {
        let mut plain = HellhoundSchedule::new(6);
        let mut seamed = HellhoundSchedule::new(6);
        for r in 1..=40 {
            let none = ReservedRounds::default();
            assert_eq!(hellhound_round(&mut seamed, r, 4, true, &none), plain.begin(r, 4, true), "round {r}");
        }
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
