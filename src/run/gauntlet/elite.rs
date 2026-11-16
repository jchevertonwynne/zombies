//! Elites: an ordinary Zombie that gets one of five Elite Traits as it spawns, from Round
//! `GAUNTLET_ELITE_FIRST_ROUND`. They're ordinary Zombies in every other way (the Nuke and
//! Insta-Kill kill them, Procs hit them), so Gauntlet layers on top of Classic through its seams
//! only (ADRs 0002 and 0007): `ZombieSpawned` marks them, `DamageTakenFactor` armours them,
//! `HealZombie` regenerates them, `ZombieMoveFactor` rallies the Zombies around them,
//! `DropChanceFactor` makes their kills drop more, `SpawnZombie` splits them and an `Explosion`
//! that hurts no Zombie blows a Volatile one up. A kill pays its kill Points again as `BonusPoints`.
//! Turning an Elite strips its trait.

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

use crate::audio::PlayCue;
use crate::audio::cues::Cue;
use crate::economy::points::{BonusPoints, kill_points};
use crate::enemies::request::{RequestedKind, SpawnAt, SpawnProfile, SpawnZombie};
use crate::enemies::target::Players;
use crate::enemies::zombie::{
    DamageTakenFactor, DamageZombie, DropChanceFactor, Dying, HealZombie, HitZone, Zombie, ZombieKilled, ZombieKind,
    ZombieMoveFactor, ZombieSpawned, healed_health, taken_damage,
};
use crate::modifier::Source;
use crate::run::round::{Round, zombie_health};
use crate::state::InRun;
use crate::tuning::*;
use crate::weapons::ammo_mod::AmmoMods;
use crate::weapons::arsenal::Splash;
use crate::weapons::explosive::Explosion;

/// The five Elite Traits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EliteTrait {
    /// Body hits do a quarter damage until its armour has soaked half its max health.
    Armoured,
    /// On death, two Walkers at 30% of its max health each.
    Splitter,
    /// On death, explodes after a short fuse, hurting the Player only.
    Volatile,
    /// Heals once it has gone a while without damage.
    Regenerating,
    /// Other Zombies near it move faster.
    Rallying,
}

impl EliteTrait {
    pub const ALL: [EliteTrait; 5] = [
        EliteTrait::Armoured,
        EliteTrait::Splitter,
        EliteTrait::Volatile,
        EliteTrait::Regenerating,
        EliteTrait::Rallying,
    ];

    /// The trait a uniform `roll` in [0, 1) draws.
    pub fn from_roll(roll: f64) -> Self {
        let i = (roll.clamp(0.0, 1.0) * Self::ALL.len() as f64) as usize;
        Self::ALL[i.min(Self::ALL.len() - 1)]
    }

    /// What it goes as on the wire (see `WorldEvent::Tags`).
    pub fn code(self) -> u8 {
        self as u8
    }

    /// The trait a wire `code` names, if any.
    pub fn from_code(code: u8) -> Option<Self> {
        Self::ALL.get(usize::from(code)).copied()
    }

    /// Its emissive tint.
    pub fn tint(self) -> Color {
        let [r, g, b] = GAUNTLET_ELITE_TINTS[self as usize];
        Color::srgb(r, g, b)
    }
}

/// A Zombie that's an Elite, with its trait.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Elite(pub EliteTrait);

/// An Armoured Elite's armour, counting the damage it has soaked.
#[derive(Component, Debug, Default)]
pub struct Armour {
    soaked: f32,
}

/// A Regenerating Elite's clock: seconds since it last took damage, and the health it had.
#[derive(Component, Debug)]
pub struct Regen {
    since_hit: f32,
    last: f32,
}

/// A dead Volatile Elite's fuse, burning down where it fell.
#[derive(Component, Debug)]
pub struct Fuse {
    at: Vec3,
    left: f32,
}

/// The source the Rallying speed-up goes on a Zombie's `ZombieMoveFactor` under.
pub const RALLYING: Source = Source("Rallying");

// ----- the pure rules -----

/// The chance a Zombie spawned in `round` is an Elite: none before the first Round, then a base
/// chance growing each Round, capped.
#[cfg(test)]
pub fn elite_chance(round: u32) -> f64 {
    elite_chance_under(round, &EliteRules::default())
}

/// What decides who becomes an Elite: the Run's chance factor, its cap, the first Round they
/// appear in and how many can be alive at once. Heat's Elite Surge changes them; the default is
/// the base rules.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct EliteRules {
    pub chance_factor: f64,
    pub max_chance: f64,
    pub first_round: u32,
    pub max_alive: u32,
}

impl Default for EliteRules {
    fn default() -> Self {
        Self {
            chance_factor: 1.0,
            max_chance: GAUNTLET_ELITE_CHANCE_MAX,
            first_round: GAUNTLET_ELITE_FIRST_ROUND,
            max_alive: GAUNTLET_ELITE_MAX_ALIVE,
        }
    }
}

/// `elite_chance` under `rules`: the grown chance from the rules' first Round, times their factor,
/// capped.
pub fn elite_chance_under(round: u32, rules: &EliteRules) -> f64 {
    if round < rules.first_round {
        return 0.0;
    }
    let grown = GAUNTLET_ELITE_CHANCE_BASE + GAUNTLET_ELITE_CHANCE_PER_ROUND * f64::from(round - rules.first_round);
    (grown * rules.chance_factor).min(rules.max_chance)
}

/// Whether a Zombie of `kind` can be an Elite: an ordinary Zombie that the Horde (not a
/// `SpawnZombie` request) sent. Hellhounds, Special Zombies, Nova Crawlers, Bosses and requested
/// Zombies (Splitter Walkers) never are.
pub fn can_be_elite(kind: ZombieKind, requested: bool) -> bool {
    kind == ZombieKind::Normal && !requested
}

/// Whether a spawn becomes an Elite, given a uniform `roll` in [0, 1) and how many Elites are
/// `alive` already.
#[cfg(test)]
pub fn becomes_elite(round: u32, kind: ZombieKind, requested: bool, alive: u32, roll: f64) -> bool {
    becomes_elite_under(round, kind, requested, alive, roll, &EliteRules::default())
}

/// `becomes_elite` under `rules`.
pub fn becomes_elite_under(round: u32, kind: ZombieKind, requested: bool, alive: u32, roll: f64, rules: &EliteRules) -> bool {
    can_be_elite(kind, requested) && alive < rules.max_alive && roll < elite_chance_under(round, rules)
}

/// The factor Armoured puts on a Zombie.
pub const ARMOUR: DamageTakenFactor =
    DamageTakenFactor { body: GAUNTLET_ELITE_ARMOURED_BODY_FACTOR, head: 1.0 };

/// What the armour soaks from a hit of `damage` to `zone`.
pub fn soaked_by(damage: f32, zone: HitZone) -> f32 {
    damage - taken_damage(damage, zone, Some(&ARMOUR))
}

/// Whether armour that has soaked `soaked` is spent, on an Elite of `max_health`.
pub fn armour_spent(soaked: f32, max_health: f32) -> bool {
    soaked >= max_health * GAUNTLET_ELITE_ARMOURED_SOAK_FRACTION
}

/// A Regenerating Elite's clock over `dt`: seconds since its last damage (`hurt` resets them), and
/// how much it heals this step, `GAUNTLET_ELITE_REGEN_FRACTION_PER_SEC` of its max health a second
/// once `GAUNTLET_ELITE_REGEN_DELAY_SECS` have passed.
pub fn regen_step(since_hit: f32, hurt: bool, dt: f32, max_health: f32) -> (f32, f32) {
    let since = if hurt { 0.0 } else { since_hit + dt };
    let heal = if since >= GAUNTLET_ELITE_REGEN_DELAY_SECS {
        max_health * GAUNTLET_ELITE_REGEN_FRACTION_PER_SEC * dt
    } else {
        0.0
    };
    (since, heal)
}

/// Whether a Zombie at `at` is rallied by one of the `ralliers` (entity and position), another
/// Zombie within `GAUNTLET_ELITE_RALLY_RADIUS_M`.
pub fn is_rallied(zombie: Entity, at: Vec3, ralliers: &[(Entity, Vec3)]) -> bool {
    ralliers
        .iter()
        .any(|&(rallier, position)| rallier != zombie && position.distance(at) <= GAUNTLET_ELITE_RALLY_RADIUS_M)
}

/// The bonus an Elite's kill pays on top of its kill Points, so the whole pays
/// `GAUNTLET_ELITE_POINTS_FACTOR` times.
pub fn elite_bonus(kill_points: u64) -> u64 {
    kill_points.saturating_mul(GAUNTLET_ELITE_POINTS_FACTOR - 1)
}

/// The Walkers a dead Splitter of `max_health` splits into in `round`, one request each, at `at`.
pub fn split_requests(round: u32, max_health: f32, at: Vec3) -> Vec<SpawnZombie> {
    let health_factor = max_health * GAUNTLET_ELITE_SPLITTER_HEALTH_FRACTION / zombie_health(round) as f32;
    (0..GAUNTLET_ELITE_SPLITTER_COUNT)
        .map(|i| {
            // Spread round where it fell, a body-width apart, so they don't spawn on one spot.
            let angle = std::f32::consts::TAU * i as f32 / GAUNTLET_ELITE_SPLITTER_COUNT as f32;
            SpawnZombie {
                kind: RequestedKind::Walker,
                health_factor,
                scale: 1.0,
                at: SpawnAt::Point(at + Vec3::new(angle.cos(), 0.0, angle.sin()) * 0.5),
                holds_round: true,
                profile: SpawnProfile::default(),
                heavyweight: None,
            }
        })
        .collect()
}

/// The blast a Volatile's fuse sets off at `at`, for each of `players`: it hurts that Player (up to
/// `GAUNTLET_ELITE_VOLATILE_DAMAGE`, within `GAUNTLET_ELITE_VOLATILE_RADIUS_M`) and no Zombie. With
/// no Player there's still the blast to see.
pub fn volatile_blasts(at: Vec3, players: &[Entity]) -> Vec<Explosion> {
    let blast = |thrower, hurts_player| Explosion {
        at,
        thrower,
        splash: Splash { max: GAUNTLET_ELITE_VOLATILE_DAMAGE, min: 0.0, radius: GAUNTLET_ELITE_VOLATILE_RADIUS_M },
        hurts_player,
        hurts_zombies: false,
        ammo_mods: AmmoMods::NONE,
        crawlers: false,
    };
    if players.is_empty() {
        return vec![blast(None, false)];
    }
    players.iter().map(|&player| blast(Some(player), true)).collect()
}

// ----- the systems -----

/// Marks each Horde Zombie that spawns as an Elite by chance, to the cap, with its trait's seam
/// components, and cues its spawn.
pub fn mark_elites(
    mut commands: Commands,
    mut spawned: MessageReader<ZombieSpawned>,
    rules: Res<EliteRules>,
    alive: Query<(), (With<Elite>, With<Zombie>, Without<Dying>)>,
    bodies: Query<(&Zombie, &Transform)>,
    mut cues: MessageWriter<PlayCue>,
) {
    let mut alive = alive.iter().count() as u32;
    for spawn in spawned.read() {
        if !becomes_elite_under(spawn.round, spawn.kind, spawn.requested, alive, rand::random(), &rules) {
            continue;
        }
        let Ok((zombie, transform)) = bodies.get(spawn.zombie) else {
            continue;
        };
        alive += 1;
        let elite = EliteTrait::from_roll(rand::random());
        let mut entity = commands.entity(spawn.zombie);
        entity.insert((Elite(elite), DropChanceFactor(GAUNTLET_ELITE_DROP_FACTOR)));
        match elite {
            EliteTrait::Armoured => {
                entity.insert((ARMOUR, Armour::default()));
            }
            EliteTrait::Regenerating => {
                entity.insert(Regen { since_hit: 0.0, last: zombie.health() });
            }
            EliteTrait::Splitter | EliteTrait::Volatile | EliteTrait::Rallying => {}
        }
        cues.write(PlayCue::new(Cue::EliteSpawn, Some(transform.translation)));
    }
}

/// Dresses each new Elite in its trait's emissive tint, on its body's material (the rig's parts
/// follow it).
pub fn tint_elites(
    mut new: Query<(&Elite, &mut MeshMaterial3d<StandardMaterial>), Added<Elite>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
    mut tints: Local<Vec<Handle<StandardMaterial>>>,
) {
    let Some(mut materials) = materials else {
        return;
    };
    if tints.is_empty() {
        *tints = EliteTrait::ALL
            .iter()
            .map(|elite| {
                materials.add(StandardMaterial {
                    base_color: elite.tint().mix(&Color::srgb(0.35, 0.4, 0.3), 0.6),
                    emissive: elite.tint().to_linear() * 2.5,
                    perceptual_roughness: 0.8,
                    ..default()
                })
            })
            .collect();
    }
    for (elite, mut material) in &mut new {
        material.0 = tints[elite.0 as usize].clone();
    }
}

/// An Armoured Elite's armour soaks the share of each hit its factor takes, and falls away once
/// it has soaked half the Elite's max health.
pub fn soak_armour(
    mut commands: Commands,
    mut hits: MessageReader<DamageZombie>,
    mut armoured: Query<(&Zombie, &mut Armour), Without<Dying>>,
) {
    for hit in hits.read() {
        let Ok((zombie, mut armour)) = armoured.get_mut(hit.zombie) else {
            continue;
        };
        if armour_spent(armour.soaked, zombie.max_health()) {
            continue;
        }
        armour.soaked += soaked_by(hit.damage, hit.zone);
        if armour_spent(armour.soaked, zombie.max_health()) {
            commands.entity(hit.zombie).remove::<(DamageTakenFactor, Armour)>();
        }
    }
}

/// A Regenerating Elite heals once it has gone `GAUNTLET_ELITE_REGEN_DELAY_SECS` without damage.
pub fn regenerate(
    mut elites: Query<(Entity, &Zombie, &mut Regen), Without<Dying>>,
    mut heals: MessageWriter<HealZombie>,
    time: Res<Time>,
) {
    for (entity, zombie, mut regen) in &mut elites {
        if zombie.health() <= 0.0 {
            continue;
        }
        let hurt = zombie.health() < regen.last - 1e-3;
        let (since, heal) = regen_step(regen.since_hit, hurt, time.delta_secs(), zombie.max_health());
        regen.since_hit = since;
        regen.last = healed_health(zombie.health(), zombie.max_health(), heal);
        if heal > 0.0 && zombie.health() < zombie.max_health() {
            heals.write(HealZombie { zombie: entity, amount: heal });
        }
    }
}

/// Every Zombie within a Rallying Elite's radius moves faster, its own source so two ralliers don't
/// stack; one that leaves it, or whose rallier dies or is Turned, goes back to its pace.
pub fn rally(
    mut commands: Commands,
    ralliers: Query<(Entity, &Elite, &GlobalTransform), (With<Zombie>, Without<Dying>)>,
    mut zombies: Query<(Entity, &GlobalTransform, Option<&mut ZombieMoveFactor>), (With<Zombie>, Without<Dying>)>,
) {
    let ralliers: Vec<(Entity, Vec3)> = ralliers
        .iter()
        .filter(|(_, elite, _)| elite.0 == EliteTrait::Rallying)
        .map(|(entity, _, at)| (entity, at.translation()))
        .collect();
    for (entity, at, factor) in &mut zombies {
        let rallied = is_rallied(entity, at.translation(), &ralliers);
        match (factor, rallied) {
            (Some(mut factor), true) => factor.set(RALLYING, GAUNTLET_ELITE_RALLY_SPEED_FACTOR),
            (None, true) => {
                let mut factor = ZombieMoveFactor::default();
                factor.set(RALLYING, GAUNTLET_ELITE_RALLY_SPEED_FACTOR);
                commands.entity(entity).insert(factor);
            }
            (Some(mut factor), false) => {
                if factor.has(RALLYING) {
                    factor.clear(RALLYING);
                    if factor.is_empty() {
                        commands.entity(entity).remove::<ZombieMoveFactor>();
                    }
                }
            }
            (None, false) => {}
        }
    }
}

/// A Rallying Elite's ground ring, at the radius it rallies.
pub fn draw_rally_rings(
    ralliers: Query<(&Elite, &Zombie, &Transform), Without<Dying>>,
    mut gizmos: Gizmos,
) {
    for (elite, zombie, transform) in &ralliers {
        if elite.0 != EliteTrait::Rallying {
            continue;
        }
        let feet = transform.translation - Vec3::Y * zombie.half_height();
        let ring = Isometry3d::new(feet + Vec3::Y * 0.05, Quat::from_rotation_x(FRAC_PI_2));
        gizmos.circle(ring, GAUNTLET_ELITE_RALLY_RADIUS_M, Color::srgb(0.95, 0.1, 0.1));
    }
}

/// A dying Elite's trait plays out: a Splitter's Walkers are asked for, and a Volatile's fuse is lit
/// where it fell. A Turned Elite is no longer a `Zombie`, so neither happens.
pub fn elite_deaths(
    mut commands: Commands,
    deaths: Query<(&Elite, &Zombie, &Transform), (Added<Dying>, With<Zombie>)>,
    round: Res<Round>,
    mut spawns: MessageWriter<SpawnZombie>,
) {
    for (elite, zombie, transform) in &deaths {
        let feet = transform.translation - Vec3::Y * zombie.half_height();
        match elite.0 {
            EliteTrait::Splitter => {
                for request in split_requests(round.number, zombie.max_health(), feet) {
                    spawns.write(request);
                }
            }
            EliteTrait::Volatile => {
                commands.spawn((InRun, Fuse { at: feet + Vec3::Y * 0.5, left: GAUNTLET_ELITE_VOLATILE_FUSE_SECS }));
            }
            EliteTrait::Armoured | EliteTrait::Regenerating | EliteTrait::Rallying => {}
        }
    }
}

/// A dying Volatile Elite's puppet on a client lights a cosmetic fuse ring where it fell, which
/// burns down for as long as the Host's does and then goes out. The blast itself comes as a `Burst`.
pub fn puppet_fuses(
    mut commands: Commands,
    deaths: Query<(&Elite, &Zombie, &Transform), (Added<Dying>, With<Zombie>)>,
    mut fuses: Query<(Entity, &mut Fuse)>,
    mut gizmos: Gizmos,
    time: Res<Time>,
) {
    for (elite, zombie, transform) in &deaths {
        if elite.0 == EliteTrait::Volatile {
            let feet = transform.translation - Vec3::Y * zombie.half_height();
            commands.spawn((InRun, Fuse { at: feet + Vec3::Y * 0.5, left: GAUNTLET_ELITE_VOLATILE_FUSE_SECS }));
        }
    }
    for (entity, mut fuse) in &mut fuses {
        fuse.left -= time.delta_secs();
        if fuse.left > 0.0 {
            draw_fuse(&mut gizmos, &fuse);
        } else {
            commands.entity(entity).despawn();
        }
    }
}

/// A burning fuse's flickering ring at the blast's radius.
fn draw_fuse(gizmos: &mut Gizmos, fuse: &Fuse) {
    let flicker = 0.9 + 0.1 * (fuse.left * 30.0).sin();
    let ring = Isometry3d::new(fuse.at, Quat::from_rotation_x(FRAC_PI_2));
    gizmos.circle(ring, GAUNTLET_ELITE_VOLATILE_RADIUS_M * flicker, Color::srgb(1.0, 0.45, 0.05));
}

/// A lit fuse burns down, then goes off as a blast at each Player that hurts no Zombie.
pub fn burn_fuses(
    mut commands: Commands,
    mut fuses: Query<(Entity, &mut Fuse)>,
    players: Players,
    mut blasts: MessageWriter<Explosion>,
    mut gizmos: Gizmos,
    time: Res<Time>,
) {
    for (entity, mut fuse) in &mut fuses {
        fuse.left -= time.delta_secs();
        if fuse.left > 0.0 {
            draw_fuse(&mut gizmos, &fuse);
            continue;
        }
        let up: Vec<Entity> = players.iter().filter(|player| player.up).map(|player| player.entity).collect();
        for blast in volatile_blasts(fuse.at, &up) {
            blasts.write(blast);
        }
        commands.entity(entity).despawn();
    }
}

/// An Elite's kill pays its kill Points again, to the Player who made it (none for an Unpaid kill).
pub fn elite_points(
    mut kills: MessageReader<ZombieKilled>,
    elites: Query<(), With<Elite>>,
    mut bonus: MessageWriter<BonusPoints>,
) {
    for kill in kills.read() {
        if let Some(player) = kill.player
            && elites.contains(kill.zombie)
        {
            let amount = elite_bonus(kill_points(kill));
            if amount > 0 {
                bonus.write(BonusPoints { player, amount });
            }
        }
    }
}

/// An Elite that was Turned (it's no longer a `Zombie`) loses its trait, and what the trait put on it.
pub fn strip_turned_elites(
    mut commands: Commands,
    turned: Query<(Entity, Has<Armour>), (With<Elite>, Without<Zombie>)>,
) {
    for (entity, armoured) in &turned {
        let mut entity = commands.entity(entity);
        entity.remove::<(Elite, Armour, Regen, DropChanceFactor)>();
        if armoured {
            entity.remove::<DamageTakenFactor>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enemies::special::SpecialKind;
    use crate::enemies::zombie::{Attack, Pay};
    use crate::run::round::zombie_health;

    #[test]
    fn the_elite_chance_is_two_percent_at_round_six_growing_to_a_cap_of_fifteen() {
        assert_eq!(elite_chance(1), 0.0);
        assert_eq!(elite_chance(5), 0.0);
        assert_eq!(elite_chance(6), 0.02);
        assert!((elite_chance(10) - 0.04).abs() < 1e-12);
        assert!((elite_chance(16) - 0.07).abs() < 1e-12);
        assert!((elite_chance(32) - 0.15).abs() < 1e-12);
        assert_eq!(elite_chance(100), 0.15);
    }

    #[test]
    fn only_the_hordes_ordinary_zombies_can_be_elites() {
        assert!(can_be_elite(ZombieKind::Normal, false));
        assert!(!can_be_elite(ZombieKind::Normal, true), "a requested Zombie (a Splitter's Walker) never is");
        assert!(!can_be_elite(ZombieKind::Hellhound, false));
        assert!(!can_be_elite(ZombieKind::NovaCrawler, false));
        for kind in [SpecialKind::Napalm, SpecialKind::Shrieker, SpecialKind::Sapper] {
            assert!(!can_be_elite(ZombieKind::Special(kind), false), "{kind:?}");
        }
    }

    #[test]
    fn at_most_three_elites_are_alive_at_once() {
        let normal = ZombieKind::Normal;
        assert!(becomes_elite(10, normal, false, 2, 0.0));
        assert!(!becomes_elite(10, normal, false, 3, 0.0));
        assert!(!becomes_elite(10, normal, false, 4, 0.0));
    }

    #[test]
    fn a_roll_under_the_chance_makes_an_elite_and_never_before_round_six() {
        let normal = ZombieKind::Normal;
        assert!(becomes_elite(6, normal, false, 0, 0.019));
        assert!(!becomes_elite(6, normal, false, 0, 0.021));
        assert!(!becomes_elite(5, normal, false, 0, 0.0));
        assert!(!becomes_elite(10, ZombieKind::Hellhound, false, 0, 0.0));
    }

    #[test]
    fn the_five_traits_are_drawn_uniformly() {
        let mut seen = [0; 5];
        for i in 0..1000 {
            seen[EliteTrait::from_roll(f64::from(i) / 1000.0) as usize] += 1;
        }
        assert_eq!(seen, [200; 5]);
        assert_eq!(EliteTrait::from_roll(1.0), EliteTrait::Rallying, "a roll can't run off the end");
    }

    #[test]
    fn armour_takes_body_hits_to_a_quarter_but_not_headshots() {
        assert_eq!(soaked_by(100.0, HitZone::Torso), 75.0);
        assert_eq!(soaked_by(100.0, HitZone::Head), 0.0);
        assert_eq!(ARMOUR, DamageTakenFactor { body: 0.25, head: 1.0 });
    }

    #[test]
    fn armour_is_spent_once_it_has_soaked_half_the_max_health() {
        assert!(!armour_spent(499.0, 1000.0));
        assert!(armour_spent(500.0, 1000.0));
    }

    #[test]
    fn regeneration_starts_after_three_seconds_without_damage_at_ten_percent_a_second() {
        let (mut since, mut healed) = (0.0, 0.0);
        for _ in 0..30 {
            let (next, heal) = regen_step(since, false, 0.1, 1000.0);
            since = next;
            healed += heal;
        }
        assert!(since >= 3.0 - 1e-4);
        // Only the last step of the 3 s was long enough ago to heal, if any.
        assert!(healed <= 10.0 + 1e-3, "{healed}");
        let (_, heal) = regen_step(3.0, false, 1.0, 1000.0);
        assert_eq!(heal, 100.0);
        assert_eq!(regen_step(2.9, false, 0.05, 1000.0).1, 0.0, "not yet");
    }

    #[test]
    fn damage_resets_the_regeneration_clock() {
        assert_eq!(regen_step(10.0, true, 0.1, 1000.0), (0.0, 0.0));
    }

    #[test]
    fn rallying_reaches_other_zombies_within_six_metres() {
        let mut world = World::new();
        let (rallier, other) = (world.spawn_empty().id(), world.spawn_empty().id());
        let ralliers = [(rallier, Vec3::ZERO)];
        assert!(is_rallied(other, Vec3::new(6.0, 0.0, 0.0), &ralliers));
        assert!(!is_rallied(other, Vec3::new(6.1, 0.0, 0.0), &ralliers));
        assert!(!is_rallied(rallier, Vec3::new(1.0, 0.0, 0.0), &ralliers), "never itself");
    }

    #[test]
    fn an_elite_kill_pays_its_kill_points_twice_over() {
        assert_eq!(elite_bonus(100), 100);
        assert_eq!(elite_bonus(0), 0, "an Unpaid kill pays nothing, doubled or not");
    }

    #[test]
    fn a_splitter_asks_for_two_walkers_at_thirty_percent_of_its_health_each() {
        let max = zombie_health(10) as f32;
        let requests = split_requests(10, max, Vec3::new(3.0, 0.0, 4.0));
        assert_eq!(requests.len(), 2);
        for request in &requests {
            assert_eq!(request.kind, RequestedKind::Walker);
            assert!(request.holds_round);
            assert_eq!(request.heavyweight, None);
            let health = crate::enemies::request::requested_health(10, request.health_factor);
            assert!((health - max * 0.3).abs() < 0.01, "{health}");
            let SpawnAt::Point(at) = request.at else { panic!("a point") };
            assert!(at.distance(Vec3::new(3.0, 0.0, 4.0)) < 1.0);
        }
    }

    #[test]
    fn a_volatile_blast_hurts_the_player_up_to_sixty_within_three_metres_and_no_zombie() {
        let player = World::new().spawn_empty().id();
        let blasts = volatile_blasts(Vec3::ZERO, &[player]);
        assert_eq!(blasts.len(), 1);
        let blast = blasts[0];
        assert_eq!(blast.thrower, Some(player));
        assert!(blast.hurts_player && !blast.hurts_zombies);
        assert_eq!(blast.splash.damage(0.0), Some(60.0));
        assert_eq!(blast.splash.damage(3.1), None);
        assert!(!blast.crawlers && blast.ammo_mods.is_empty());
        let alone = volatile_blasts(Vec3::ZERO, &[]);
        assert!(alone.len() == 1 && !alone[0].hurts_player && !alone[0].hurts_zombies);
    }

    // ----- through the systems -----

    fn elite_app() -> App {
        let mut app = App::new();
        app.add_message::<ZombieSpawned>()
            .add_message::<PlayCue>()
            .add_message::<DamageZombie>()
            .add_message::<HealZombie>()
            .add_message::<SpawnZombie>()
            .add_message::<ZombieKilled>()
            .add_message::<BonusPoints>()
            .init_resource::<EliteRules>()
            .insert_resource(Time::<()>::default());
        app
    }

    fn drain<M: Message>(app: &mut App) -> Vec<M> {
        app.world_mut().resource_mut::<Messages<M>>().drain().collect()
    }

    fn spawn_event(zombie: Entity, round: u32, requested: bool) -> ZombieSpawned {
        ZombieSpawned { zombie, kind: ZombieKind::Normal, round, requested }
    }

    #[test]
    fn a_round_before_six_marks_no_elite_and_a_certain_chance_marks_one_per_spawn_to_the_cap() {
        // At the cap of chance (0.15) a lucky roll can't be forced, so drive the cap rule: three
        // Elites already alive means a fourth is refused whatever the roll.
        let mut app = elite_app();
        app.add_systems(Update, mark_elites);
        for _ in 0..3 {
            app.world_mut().spawn((Elite(EliteTrait::Splitter), Zombie::new_for_test(100.0)));
        }
        let fourth = app.world_mut().spawn((Zombie::new_for_test(100.0), Transform::default())).id();
        for _ in 0..200 {
            app.world_mut().write_message(spawn_event(fourth, 40, false));
        }
        app.update();
        assert!(app.world().get::<Elite>(fourth).is_none());
        // And none before Round 6, however many spawn.
        let early = app.world_mut().spawn((Zombie::new_for_test(100.0), Transform::default())).id();
        app.world_mut().despawn(fourth);
        for _ in 0..200 {
            app.world_mut().write_message(spawn_event(early, 5, false));
        }
        app.update();
        assert!(app.world().get::<Elite>(early).is_none());
    }

    #[test]
    fn an_elite_marked_at_spawn_has_its_trait_its_drop_factor_and_a_cue() {
        let mut app = elite_app();
        app.add_systems(Update, mark_elites);
        // Many spawns in Round 100: one chance in about seven each, so some become Elites, never
        // more than the cap.
        let zombies: Vec<Entity> =
            (0..60).map(|_| app.world_mut().spawn((Zombie::new_for_test(100.0), Transform::default())).id()).collect();
        for &zombie in &zombies {
            app.world_mut().write_message(spawn_event(zombie, 100, false));
        }
        app.update();
        let elites: Vec<Entity> = zombies.iter().copied().filter(|&z| app.world().get::<Elite>(z).is_some()).collect();
        assert!(elites.len() <= 3, "{} Elites", elites.len());
        for &elite in &elites {
            assert_eq!(app.world().get::<DropChanceFactor>(elite), Some(&DropChanceFactor(3.0)));
        }
        assert_eq!(drain::<PlayCue>(&mut app).len(), elites.len(), "one spawn cue each");
    }

    #[test]
    fn an_armoured_elite_loses_its_armour_after_soaking_half_its_health() {
        let mut app = elite_app();
        app.add_systems(Update, soak_armour);
        let elite = app
            .world_mut()
            .spawn((Elite(EliteTrait::Armoured), ARMOUR, Armour::default(), Zombie::new_for_test(1000.0)))
            .id();
        let hit = |damage| DamageZombie { player: None, zombie: elite, damage, zone: HitZone::Torso, attack: Attack::Bullet };
        // 400 damage soaks 300.
        app.world_mut().write_message(hit(400.0));
        app.update();
        assert!(app.world().get::<DamageTakenFactor>(elite).is_some());
        // 300 more soaks 225: past 500 in all.
        app.world_mut().write_message(hit(300.0));
        app.update();
        assert!(app.world().get::<DamageTakenFactor>(elite).is_none(), "the armour is gone");
        assert!(app.world().get::<Armour>(elite).is_none());
    }

    #[test]
    fn headshots_never_wear_armour_down() {
        let mut app = elite_app();
        app.add_systems(Update, soak_armour);
        let elite = app
            .world_mut()
            .spawn((Elite(EliteTrait::Armoured), ARMOUR, Armour::default(), Zombie::new_for_test(1000.0)))
            .id();
        app.world_mut().write_message(DamageZombie {
            player: None,
            zombie: elite,
            damage: 5000.0,
            zone: HitZone::Head,
            attack: Attack::Bullet,
        });
        app.update();
        assert!(app.world().get::<DamageTakenFactor>(elite).is_some());
    }

    fn step(app: &mut App, secs: f32) {
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs_f32(secs));
        app.update();
    }

    #[test]
    fn a_regenerating_elite_heals_after_three_quiet_seconds_and_not_while_being_hurt() {
        let mut app = elite_app();
        app.add_systems(Update, regenerate);
        let elite = app
            .world_mut()
            .spawn((Elite(EliteTrait::Regenerating), Regen { since_hit: 0.0, last: 1000.0 }, Zombie::new_for_test(1000.0)))
            .id();
        for _ in 0..2 {
            step(&mut app, 1.0);
        }
        assert!(drain::<HealZombie>(&mut app).is_empty(), "two seconds in, nothing yet");
        step(&mut app, 1.0);
        step(&mut app, 1.0);
        let heals = drain::<HealZombie>(&mut app);
        assert!(heals.iter().all(|heal| heal.zombie == elite));
        // Hurt: nothing from there on.
        app.world_mut().get_mut::<Zombie>(elite).unwrap().set_health_for_test(500.0);
        step(&mut app, 1.0);
        assert!(drain::<HealZombie>(&mut app).is_empty(), "damage resets the clock");
    }

    #[test]
    fn a_rallier_speeds_the_zombies_near_it_and_lets_go_of_those_that_leave() {
        let mut app = elite_app();
        app.add_systems(Update, rally);
        let rallier = app
            .world_mut()
            .spawn((Elite(EliteTrait::Rallying), Zombie::new_for_test(100.0), GlobalTransform::default()))
            .id();
        let near = app.world_mut().spawn((Zombie::new_for_test(100.0), GlobalTransform::from_xyz(5.0, 0.0, 0.0))).id();
        let far = app.world_mut().spawn((Zombie::new_for_test(100.0), GlobalTransform::from_xyz(8.0, 0.0, 0.0))).id();
        app.update();
        let factor = |app: &App, zombie| app.world().get::<ZombieMoveFactor>(zombie).map(ZombieMoveFactor::factor);
        assert_eq!(factor(&app, near), Some(1.25));
        assert_eq!(factor(&app, far), None);
        assert_eq!(factor(&app, rallier), None, "not itself");
        // It dies: the Zombie it rallied goes back to its own pace.
        app.world_mut().entity_mut(rallier).insert(Dying::default());
        app.update();
        assert_eq!(factor(&app, near), None);
    }

    #[test]
    fn a_dying_splitter_asks_for_walkers_and_a_turned_one_does_not() {
        let mut app = elite_app();
        app.insert_resource(Round::in_progress(10)).add_systems(Update, elite_deaths);
        let splitter = app
            .world_mut()
            .spawn((Elite(EliteTrait::Splitter), Zombie::new_for_test(500.0), Transform::default()))
            .id();
        app.world_mut().entity_mut(splitter).insert(Dying::default());
        app.update();
        let spawns = drain::<SpawnZombie>(&mut app);
        assert_eq!(spawns.len(), 2);
        assert!(spawns.iter().all(|request| request.kind == RequestedKind::Walker));

        // A Turned one has lost its Zombie before it falls.
        let turned = app.world_mut().spawn((Elite(EliteTrait::Splitter), Transform::default())).id();
        app.world_mut().entity_mut(turned).insert(Dying::default());
        app.update();
        assert!(drain::<SpawnZombie>(&mut app).is_empty());
    }

    #[test]
    fn a_dying_volatile_lights_a_one_second_fuse_where_it_fell() {
        let mut app = elite_app();
        app.insert_resource(Round::in_progress(10)).add_systems(Update, elite_deaths);
        let volatile = app
            .world_mut()
            .spawn((Elite(EliteTrait::Volatile), Zombie::new_for_test(500.0), Transform::default()))
            .id();
        app.world_mut().entity_mut(volatile).insert(Dying::default());
        app.update();
        let fuses: Vec<_> = app.world_mut().query::<&Fuse>().iter(app.world()).map(|fuse| fuse.left).collect();
        assert_eq!(fuses, [GAUNTLET_ELITE_VOLATILE_FUSE_SECS]);
    }

    #[test]
    fn an_elite_kill_pays_bonus_points_to_its_killer_and_a_plain_kill_pays_none() {
        let mut app = elite_app();
        app.add_systems(Update, elite_points);
        let player = app.world_mut().spawn_empty().id();
        let elite = app.world_mut().spawn(Elite(EliteTrait::Armoured)).id();
        let plain = app.world_mut().spawn_empty().id();
        let kill = |zombie, pay| ZombieKilled {
            player: Some(player),
            pay,
            headshot: true,
            zone: HitZone::Head,
            attack: Attack::Bullet,
            at: Vec3::ZERO,
            surge: false,
            zombie,
        };
        app.world_mut().write_message(kill(elite, Pay::Paid));
        app.world_mut().write_message(kill(plain, Pay::Paid));
        app.world_mut().write_message(kill(elite, Pay::Unpaid));
        app.update();
        let paid: Vec<_> = drain::<BonusPoints>(&mut app).iter().map(|bonus| (bonus.player, bonus.amount)).collect();
        assert_eq!(paid, [(player, 100)], "a headshot's 100 again, for the Elite's Paid kill only");
    }

    #[test]
    fn turning_an_elite_strips_its_trait() {
        let mut app = elite_app();
        app.add_systems(Update, strip_turned_elites);
        let turned = app
            .world_mut()
            .spawn((Elite(EliteTrait::Armoured), ARMOUR, Armour::default(), DropChanceFactor(3.0)))
            .id();
        let live = app.world_mut().spawn((Elite(EliteTrait::Rallying), Zombie::new_for_test(10.0))).id();
        app.update();
        let world = app.world();
        assert!(world.get::<Elite>(turned).is_none());
        assert!(world.get::<DamageTakenFactor>(turned).is_none());
        assert!(world.get::<DropChanceFactor>(turned).is_none());
        assert!(world.get::<Elite>(live).is_some());
    }

    #[test]
    fn the_volatile_fuse_system_has_no_clashing_queries() {
        fn set_up<M>(system: impl IntoSystem<(), (), M>) {
            let mut world = World::new();
            IntoSystem::into_system(system).initialize(&mut world);
        }
        set_up(mark_elites);
        set_up(tint_elites);
        set_up(soak_armour);
        set_up(regenerate);
        set_up(rally);
        set_up(draw_rally_rings);
        set_up(elite_deaths);
        set_up(burn_fuses);
        set_up(elite_points);
        set_up(strip_turned_elites);
    }

    #[test]
    fn elite_surge_grows_the_chance_from_its_own_first_round_to_its_own_cap() {
        let one = EliteRules { chance_factor: 1.5, max_chance: 0.225, first_round: 6, max_alive: 3 };
        assert_eq!(elite_chance_under(5, &one), 0.0);
        assert!((elite_chance_under(6, &one) - 0.03).abs() < 1e-12);
        assert_eq!(elite_chance_under(100, &one), 0.225);
        let two = EliteRules { chance_factor: 2.0, max_chance: 0.30, first_round: 3, max_alive: 5 };
        assert_eq!(elite_chance_under(2, &two), 0.0);
        assert!((elite_chance_under(3, &two) - 0.04).abs() < 1e-12, "Elites from Round 3");
        assert_eq!(elite_chance_under(100, &two), 0.30);
    }

    #[test]
    fn elite_surge_raises_the_alive_cap() {
        let two = EliteRules { chance_factor: 2.0, max_chance: 0.30, first_round: 3, max_alive: 5 };
        let normal = ZombieKind::Normal;
        assert!(becomes_elite_under(10, normal, false, 4, 0.0, &two));
        assert!(!becomes_elite_under(10, normal, false, 5, 0.0, &two));
        assert!(!becomes_elite_under(10, normal, false, 3, 0.0, &EliteRules::default()), "the base cap is 3");
    }

    #[test]
    fn the_default_elite_rules_are_the_base_rules() {
        let base = EliteRules::default();
        for round in [1, 6, 10, 32, 100] {
            assert_eq!(elite_chance_under(round, &base), elite_chance(round));
        }
    }
}
