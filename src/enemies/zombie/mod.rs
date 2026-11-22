use std::f32::consts::FRAC_PI_2;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_transform_interpolation::{TransformEasingSystems, TranslationEasingState};

use crate::authority::HostSimulation;
use crate::maps::door::Door;
use crate::layers::GameLayer;
use crate::modifier::{Modifiers, Source, Stat, resolve};
use crate::players::player::LocalPlayer;
use crate::maps::barrier::{Barriers, Entering};
use crate::maps::map::MapId;
use crate::weapons::ammo_mod::TurnedAlly;
use crate::weapons::explosive::in_sight;
use crate::enemies::hellhound::Telegraph;
use crate::enemies::nav::{NavGraph, NavState, Route, floor_ledge, repath_interval};
use crate::enemies::target::{Players, nearest_up_player};
use crate::enemies::nova::NovaCrawlerKilled;
use crate::players::player::{HitGrace, PlayerHit};
use crate::economy::power_up::{ActivePowerUps, PowerUp};
use crate::run::round::{CarriesOver, Round};
use crate::maps::surge::Surge;
use crate::enemies::request::Requested;
use crate::enemies::special::{SpecialCue, SpecialKind, hit_pays};
use crate::audio::cues::Cue;
use crate::state::{InRun, world_runs};
use crate::tuning::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeedTier {
    Walker,
    Jogger,
    Sprinter,
}

impl SpeedTier {
    pub fn speed(self) -> f32 {
        match self {
            SpeedTier::Walker => ZOMBIE_WALK_SPEED,
            SpeedTier::Jogger => ZOMBIE_JOG_SPEED,
            SpeedTier::Sprinter => ZOMBIE_SPRINT_SPEED,
        }
    }
}

/// Where on a Zombie a hit landed, which decides damage and kill Points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HitZone {
    Limb,
    Torso,
    Neck,
    Head,
}

/// What dealt the damage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attack {
    Bullet,
    Knife,
    /// Burn damage over time, from fire.
    Burn,
    /// An electric Trap's shock. The Map's kill, not the Player's.
    Trap,
    /// A Plank torn off hurting the Zombie that tore it, when a stat entry says so. The Map's
    /// kill, not the Player's.
    Barrier,
    /// The Nuke Power-up. Its kills are Unpaid.
    Nuke,
    /// The Teleporter's charge at a Pad and its arrival at the Mainframe. The Map's kill, Unpaid.
    Teleporter,
    /// An Ammo Mod's own damage, Paid or Unpaid by the Ammo Mod. It never rolls Ammo Mods.
    AmmoMod(Pay),
    /// A Napalm Zombie's explosion or its fire. Its kills are Unpaid.
    Napalm,
    /// A Shrieker's death scream. Its kills are Unpaid.
    Scream,
    /// A Nova Crawler's gas burst. Its kills are Unpaid.
    NovaBlast,
    /// A Swing Bridge tipping it into the canal. The Map's kill, Unpaid; the canal puts out a
    /// Napalm and a Shrieker, so there is no death effect.
    Swing,
}

impl Attack {
    /// Whether a kill by this attack pays kill Points.
    pub fn pay(self) -> Pay {
        match self {
            Attack::AmmoMod(pay) => pay,
            Attack::Bullet | Attack::Knife | Attack::Burn => Pay::Paid,
            Attack::Trap
            | Attack::Barrier
            | Attack::Nuke
            | Attack::Teleporter
            | Attack::NovaBlast
            | Attack::Napalm
            | Attack::Swing
            | Attack::Scream => Pay::Unpaid,
        }
    }
}

/// Whether a hit counts as a Headshot: a bullet to the head. A Knife kill pays as a Knife kill wherever it lands.
pub fn is_headshot(zone: HitZone, attack: Attack) -> bool {
    zone == HitZone::Head && attack == Attack::Bullet
}

/// Whether a kill pays kill Points. An Unpaid Kill still counts toward the Round and the Run Record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pay {
    Paid,
    Unpaid,
}

/// The damage a hit really does: all of the Zombie's health while Insta-Kill is on, so any
/// damage kills.
pub fn dealt_damage(damage: f32, insta_kill: bool) -> f32 {
    if insta_kill && damage > 0.0 { f32::INFINITY } else { damage }
}

/// The damage a hit really does to a Zombie Insta-Kill can't kill outright (the Napalm): twice
/// the damage while Insta-Kill is on.
pub fn doubled_damage(damage: f32, insta_kill: bool) -> f32 {
    if insta_kill { damage * 2.0 } else { damage }
}

/// A Zombie too big for the one-blow kills and crowd effects that end any other: the Sapper,
/// and anything layered on Classic that asks for one (ADR 0007). A Trap takes `trap_share` of its
/// max health once per entry instead of killing it; an Ammo Mod proc, or any other always-kill,
/// takes `ammo_mod_share`; a Knife kill roll takes `knife_kill_share`. The Nuke, Monkey Bombs and
/// the Teleporter pass it by, and it keeps its feet where another would be knocked down or flung.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Heavyweight {
    pub trap_share: f32,
    pub ammo_mod_share: f32,
    pub knife_kill_share: f32,
}

impl Heavyweight {
    /// The Sapper's shares.
    pub const SAPPER: Self = Self {
        trap_share: TRAP_SAPPER_HEALTH_FRACTION,
        ammo_mod_share: SAPPER_AMMO_MOD_HEALTH_FRACTION,
        knife_kill_share: SAPPER_KNIFE_KILL_HEALTH_FRACTION,
    };
}

/// The damage a hit really does to a `Heavyweight` with `max_health`. Insta-Kill only doubles it.
/// An Ammo Mod proc, which would kill anything else, deals its `ammo_mod_share` of max health
/// instead, and so does any other one-blow kill (a Napalm's blast, a death scream, a Nova
/// Crawler's burst, a Blast); a Knife kill roll deals its `knife_kill_share`.
pub fn heavyweight_damage(damage: f32, attack: Attack, max_health: f32, insta_kill: bool, shares: &Heavyweight) -> f32 {
    let damage = if attack == Attack::Knife && !damage.is_finite() {
        max_health * shares.knife_kill_share
    } else if matches!(attack, Attack::AmmoMod(_)) || !damage.is_finite() {
        max_health * shares.ammo_mod_share
    } else {
        damage
    };
    doubled_damage(damage, insta_kill)
}

/// The damage a hit really does to the Sapper, with `max_health`: `heavyweight_damage` at its
/// shares.
#[cfg(test)]
pub fn sapper_damage(damage: f32, attack: Attack, max_health: f32, insta_kill: bool) -> f32 {
    heavyweight_damage(damage, attack, max_health, insta_kill, &Heavyweight::SAPPER)
}

/// Request to damage a Zombie, written by the Player's attacks.
#[derive(Message)]
pub struct DamageZombie {
    /// The Player whose attack this is, who the hit and any kill credit. `None` for damage no
    /// Player is behind.
    pub player: Option<Entity>,
    pub zombie: Entity,
    pub damage: f32,
    pub zone: HitZone,
    pub attack: Attack,
}

/// Request to stun a Zombie in place for `secs`: it can't move or attack until it wears off. A
/// longer stun replaces a shorter one already under way. Only the Sapper is ever stunned.
#[derive(Message, Debug, Clone, Copy)]
pub struct StunZombie {
    pub zombie: Entity,
    pub secs: f32,
}

/// Factors on the damage one Zombie takes: `head` on a hit to the head, `body` on any other.
/// Absent, it takes a hit's damage as dealt. Classic never adds one; anything layered on it may
/// (ADR 0007).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct DamageTakenFactor {
    pub body: f32,
    pub head: f32,
}

impl Default for DamageTakenFactor {
    fn default() -> Self {
        Self { body: 1.0, head: 1.0 }
    }
}

/// A hit's damage to a Zombie after its `DamageTakenFactor`, if it has one, and before Insta-Kill.
pub fn taken_damage(damage: f32, zone: HitZone, factor: Option<&DamageTakenFactor>) -> f32 {
    match (factor, zone) {
        (None, _) => damage,
        (Some(factor), HitZone::Head) => damage * factor.head,
        (Some(factor), _) => damage * factor.body,
    }
}

/// Factors on one Zombie's move speed, one per source, multiplied together. Absent, or empty, it
/// moves at its usual speed. Classic never adds one; anything layered on it may (ADR 0007).
#[derive(Component, Debug, Default, Clone, PartialEq)]
pub struct ZombieMoveFactor(Vec<(Source, f32)>);

impl ZombieMoveFactor {
    /// Sets `source`'s factor, replacing any it already had.
    pub fn set(&mut self, source: Source, factor: f32) {
        match self.0.iter_mut().find(|(held, _)| *held == source) {
            Some((_, held)) => *held = factor,
            None => self.0.push((source, factor)),
        }
    }

    /// Whether `source` has a factor on.
    pub fn has(&self, source: Source) -> bool {
        self.0.iter().any(|(held, _)| *held == source)
    }

    /// Takes `source`'s factor off.
    pub fn clear(&mut self, source: Source) {
        self.0.retain(|(held, _)| *held != source);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Every source's factor, multiplied.
    pub fn factor(&self) -> f32 {
        self.0.iter().map(|(_, factor)| factor).product()
    }
}

/// The source a Zombie's wading slow goes on its `ZombieMoveFactor` under.
pub const WADING: Source = Source("Wading");

/// Zombies of every kind, the Sapper included, wade through Shallow Water at `WADE_SPEED_MULT`:
/// set from their feet every tick, and cleared as they step out. It's the ground, not a crowd
/// effect, so `Heavyweight` doesn't skip it. Nav costs don't change.
fn wade(
    mut commands: Commands,
    mut zombies: Query<(Entity, &Zombie, &Transform, Option<&mut ZombieMoveFactor>), Without<Dying>>,
    map: Option<Res<MapId>>,
    tide: Option<Res<crate::maps::tide::Tide>>,
) {
    let Some(layout) = map.map(|map| map.layout()).filter(|layout| !layout.water.is_empty()) else {
        return;
    };
    let tide = tide.map_or_else(Default::default, |tide| tide.level());
    for (entity, zombie, transform, factor) in &mut zombies {
        let feet = transform.translation - Vec3::Y * zombie.half_height();
        let in_water = crate::maps::water::wading(layout, tide, feet);
        match (factor, in_water) {
            (Some(mut factor), true) => factor.set(WADING, WADE_SPEED_MULT),
            (None, true) => {
                let mut factor = ZombieMoveFactor::default();
                factor.set(WADING, WADE_SPEED_MULT);
                commands.entity(entity).insert(factor);
            }
            (Some(mut factor), false) if !factor.is_empty() => {
                factor.clear(WADING);
                if factor.is_empty() {
                    commands.entity(entity).remove::<ZombieMoveFactor>();
                }
            }
            _ => {}
        }
    }
}

/// A Zombie's move speed of `speed`, after its move factors if it has any.
pub fn moved_speed(speed: f32, factor: Option<&ZombieMoveFactor>) -> f32 {
    speed * factor.map_or(1.0, ZombieMoveFactor::factor)
}

/// A stunned Zombie, standing still for `left` more seconds.
#[derive(Component, Debug)]
pub struct Stunned {
    left: f32,
}

/// A Zombie heading for a point of its own (feet height) rather than the Player, as the Sapper
/// does for the system it means to Sabotage. It beats a Monkey Bomb's pull.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Seeking(pub Vec3);

/// Request to knock a Zombie off its feet, as the Thundergun's blast does: it's flung along
/// `push` and lies there a moment before it gets up.
#[derive(Message)]
pub struct KnockDown {
    pub zombie: Entity,
    pub push: Vec3,
}

/// A Zombie took damage and survived.
#[derive(Message, Debug, Clone, Copy)]
pub struct ZombieHit {
    /// The Player whose attack landed it, who is paid for it.
    pub player: Option<Entity>,
    /// Whether the hit pays hit Points: a Napalm's or the Sapper's never do, a Shrieker's every 2nd.
    pub pays: bool,
    pub headshot: bool,
}

/// A Zombie's swing landed on a Turned ally. The ally takes no harm from it.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnedAllyHit {
    pub ally: Entity,
    /// The Zombie that swung.
    pub by: Entity,
}

/// An explosion blew a Zombie's legs off: it becomes a Crawler.
#[derive(Message, Debug, Clone, Copy)]
pub struct Gibbed {
    pub zombie: Entity,
}

/// The Crawler rule: whether a blast from an explosive that can make Crawlers blows a Zombie's
/// legs off. Never with Crawlers switched off in Settings (`crawlers_on` false). Only a normal
/// Zombie (not already a Crawler) that survives a hit of at least `CRAWLER_GIB_MIN_FRACTION` of
/// its pre-hit `health` can lose them, on a `roll` in `[0, 1)` under `CRAWLER_GIB_CHANCE`.
pub fn blows_legs_off(crawlers_on: bool, normal: bool, health: f32, damage: f32, roll: f64) -> bool {
    crawlers_on && normal && damage < health && damage >= health * CRAWLER_GIB_MIN_FRACTION && roll < CRAWLER_GIB_CHANCE
}

/// A Crawler can't tear Planks, so `CRAWLER_PLANKED_STUCK_SECS` spent at a planked Barrier counts
/// as stuck. Returns the time waited so far, which an open Barrier resets, and whether it's stuck.
pub fn planked_wait(waited: f32, planked: bool, dt: f32) -> (f32, bool) {
    let waited = if planked { waited + dt } else { 0.0 };
    (waited, waited >= CRAWLER_PLANKED_STUCK_SECS)
}

/// The kill message: a Zombie died. Every kill, from any source, is sent as one of these.
#[derive(Message, Debug, Clone, Copy)]
pub struct ZombieKilled {
    /// The Player whose attack killed it, who is paid and counted for it.
    pub player: Option<Entity>,
    pub pay: Pay,
    pub headshot: bool,
    pub zone: HitZone,
    pub attack: Attack,
    /// Where the Zombie's feet were when it died, for a Power-up to drop at.
    pub at: Vec3,
    /// A Surge Zombie: it pays as normal but drops no Power-up.
    pub surge: bool,
    /// The Zombie that died. A Turned Zombie's kill (Unpaid) names the ally it makes, which lives.
    pub zombie: Entity,
}

/// What kind of Zombie it is, for rules layered on Classic that care.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZombieKind {
    /// A normal Zombie of any Speed Tier, or a Crawler it became.
    Normal,
    Hellhound,
    NovaCrawler,
    /// A Napalm, a Shrieker or the Sapper.
    Special(SpecialKind),
}

impl ZombieKind {
    pub fn of(zombie: &Zombie) -> Self {
        if zombie.is_hellhound() {
            Self::Hellhound
        } else if zombie.is_nova_crawler() {
            Self::NovaCrawler
        } else if let Some(kind) = zombie.special_kind() {
            Self::Special(kind)
        } else {
            Self::Normal
        }
    }
}

/// A Zombie entered play: from the Horde, a special slot or a `SpawnZombie` (`requested`), in
/// Round `round`. It's sent once the Zombie's kind is settled, so a rule that marks Zombies as
/// they spawn can read the Zombie.
#[derive(Message, Debug, Clone, Copy)]
pub struct ZombieSpawned {
    pub zombie: Entity,
    pub kind: ZombieKind,
    pub round: u32,
    pub requested: bool,
}

/// Restores a Zombie's health, capped at its max. A dead or dying Zombie isn't healed.
#[derive(Message, Debug, Clone, Copy)]
pub struct HealZombie {
    pub zombie: Entity,
    pub amount: f32,
}

/// The health a Zombie has after a heal of `amount`: never above `max`.
pub fn healed_health(health: f32, max: f32, amount: f32) -> f32 {
    (health + amount.max(0.0)).min(max)
}

/// A Zombie's multiple of the Power-up drop chance of its kill. Absent, it's 1.0.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct DropChanceFactor(pub f32);

/// The damage side of a kill made by a hit, sent beside its `ZombieKilled` for rules that need
/// it: the dead Zombie's max Health, and the damage beyond what it took to kill it.
#[derive(Message, Debug, Clone, Copy)]
pub struct KillDamage {
    /// The Player whose attack made the kill.
    pub player: Option<Entity>,
    pub zombie: Entity,
    /// Where the Zombie's feet were when it died.
    pub at: Vec3,
    pub max_health: f32,
    /// The hit's damage past the Health the Zombie had left. Infinite for a hit that always kills.
    pub overflow: f32,
    pub attack: Attack,
    /// The killing hit's damage, as it was sent.
    pub damage: f32,
    /// Whether the killing hit was a Headshot.
    pub headshot: bool,
}

/// The damage a hit does past the Health its Zombie had left, before Insta-Kill.
pub fn overflow_damage(health: f32, damage: f32) -> f32 {
    (damage - health.max(0.0)).max(0.0)
}

/// A stuck or lost Zombie was quietly removed. It wasn't killed, so it goes back into the Horde.
#[derive(Message, Debug, Clone, Copy)]
pub struct ZombieCleanedUp;

/// Watches a Zombie for the stuck rule: no progress, or too far from the Player, for too long.
#[derive(Debug, Clone, Copy)]
pub struct StuckWatch {
    /// Where the Zombie last made progress from.
    anchor: Vec3,
    secs: f32,
    /// The route length left when `update_on_route` last saw it close in, or the most it has been.
    reference: f32,
    /// Seconds it has been far along its route and not closing in.
    lost: f32,
}

impl StuckWatch {
    pub fn new(at: Vec3) -> Self {
        Self {
            anchor: at,
            secs: 0.0,
            reference: 0.0,
            lost: 0.0,
        }
    }

    /// The stuck rule for a Zombie that follows a route to its Player: `remaining` is the length
    /// of that route from `at` to them, `speed` how fast the Zombie moves. Returns true when it
    /// should be cleaned up, which is either of:
    /// - It is wedged: it has gone `ZOMBIE_CLEANUP_SECS` without moving `ZOMBIE_PROGRESS_DIST`.
    /// - It is lost: its route is over `ZOMBIE_CLEANUP_DIST` long and it hasn't closed in on the
    ///   Player by `ZOMBIE_PROGRESS_DIST` for `lost_secs(speed)`.
    ///
    /// A Zombie crawling steadily after a Player who ran off is neither: it moves, and gets
    /// closer as soon as they stop. A Zombie at the Player is never stuck.
    pub fn update_on_route(&mut self, at: Vec3, player: Vec3, remaining: f32, speed: f32, dt: f32) -> bool {
        if at.distance(player) <= 2.0 * ZOMBIE_REACH {
            *self = Self {
                reference: remaining,
                ..Self::new(at)
            };
            return false;
        }
        if at.distance(self.anchor) >= ZOMBIE_PROGRESS_DIST {
            self.anchor = at;
            self.secs = 0.0;
        } else {
            self.secs += dt;
        }
        if remaining <= self.reference - ZOMBIE_PROGRESS_DIST {
            self.reference = remaining;
            self.lost = 0.0;
        } else if remaining > self.reference {
            self.reference = remaining;
        }
        if remaining > ZOMBIE_CLEANUP_DIST {
            self.lost += dt;
        } else {
            self.lost = 0.0;
        }
        self.secs >= ZOMBIE_CLEANUP_SECS || self.lost >= lost_secs(speed)
    }

    /// A watch about to run out on a Zombie at `at` that makes no progress.
    #[cfg(test)]
    pub fn nearly_up(at: Vec3) -> Self {
        Self {
            secs: ZOMBIE_CLEANUP_SECS - 0.05,
            ..Self::new(at)
        }
    }

    /// Advances the watch by `dt` with the Zombie at `at` and the Player at `player`. Returns true
    /// once the Zombie has gone `ZOMBIE_CLEANUP_SECS` beyond `ZOMBIE_CLEANUP_DIST` of the Player,
    /// or without moving `ZOMBIE_PROGRESS_DIST`, and should be cleaned up. A Zombie at the Player
    /// is never stuck.
    pub fn update(&mut self, at: Vec3, player: Vec3, dt: f32) -> bool {
        let to_player = at.distance(player);
        if to_player > ZOMBIE_CLEANUP_DIST {
            self.secs += dt;
        } else if to_player <= 2.0 * ZOMBIE_REACH || at.distance(self.anchor) >= ZOMBIE_PROGRESS_DIST {
            self.anchor = at;
            self.secs = 0.0;
        } else {
            self.secs += dt;
        }
        self.secs >= ZOMBIE_CLEANUP_SECS
    }
}

/// How long a Zombie moving at `speed` may be far along its route without closing in before it's
/// lost: `ZOMBIE_LOST_SECS`, or longer for a slow mover, which needs `ZOMBIE_LOST_DIST` of walking.
pub fn lost_secs(speed: f32) -> f32 {
    ZOMBIE_LOST_SECS.max(ZOMBIE_LOST_DIST / speed.max(0.1))
}

/// The length of the walk from `at` along `waypoints` and on to `target`.
pub fn route_remaining(at: Vec3, waypoints: &[Vec3], target: Vec3) -> f32 {
    let mut total = 0.0;
    let mut from = at;
    for &next in waypoints.iter().chain(std::iter::once(&target)) {
        total += from.distance(next);
        from = next;
    }
    total
}

#[derive(Component)]
pub struct Zombie {
    health: f32,
    /// The Health it spawned with.
    max_health: f32,
    tier: SpeedTier,
    attack_cooldown: f32,
    /// Seconds left before a swing lands; `None` when not swinging.
    wind_up: Option<f32>,
    /// Vertical speed, from gravity and slopes.
    fall_speed: f32,
    stuck: StuckWatch,
    /// Its legs were blown off: it crawls, low and slow.
    crawler: bool,
    /// Seconds a Crawler has waited at a planked Barrier.
    planked_wait: f32,
    /// A Hellhound: fast, low, bites, and never uses Barriers.
    hellhound: bool,
    /// A Nova Crawler: low and quick, from a Vent, and never a Crawler.
    nova: bool,
    /// A Napalm or a Shrieker, from the special slot.
    special: Option<SpecialKind>,
    /// Hits it has survived, for a Shrieker's every-2nd-hit Points.
    hits: u32,
    /// How many times the usual size its body is: 1 but for a requested Zombie asked to be bigger.
    scale: f32,
}

impl Zombie {
    fn new(health: f32, tier: SpeedTier, at: Vec3) -> Self {
        Self {
            health,
            max_health: health,
            tier,
            attack_cooldown: 0.0,
            wind_up: None,
            fall_speed: 0.0,
            stuck: StuckWatch::new(at),
            crawler: false,
            planked_wait: 0.0,
            hellhound: false,
            nova: false,
            special: None,
            hits: 0,
            scale: 1.0,
        }
    }

    /// The same Zombie with a body `scale` times the usual size, which reaches that much farther.
    pub fn scaled(self, scale: f32) -> Self {
        Self { scale, ..self }
    }

    /// Makes the body `scale` times the usual size, for a puppet of a scaled Zombie.
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
    }

    /// A normal Zombie with `health`, for tests.
    #[cfg(test)]
    pub fn new_for_test(health: f32) -> Self {
        Self::new(health, SpeedTier::Walker, Vec3::ZERO)
    }

    /// Sets its health, for tests.
    #[cfg(test)]
    pub fn set_health_for_test(&mut self, health: f32) {
        self.health = health;
    }

    /// A Hellhound with `health`, its centre at `at`.
    pub fn hellhound(health: f32, at: Vec3) -> Self {
        Self {
            hellhound: true,
            ..Self::new(health, SpeedTier::Sprinter, at)
        }
    }

    /// A Nova Crawler with `health`, its centre at `at`.
    pub fn nova_crawler(health: f32, at: Vec3) -> Self {
        Self {
            nova: true,
            ..Self::new(health, SpeedTier::Sprinter, at)
        }
    }

    /// A Napalm (a Walker), a Shrieker (a Sprinter) or the Sapper (at its own speed) with
    /// `health`, its centre at `at`.
    pub fn special(kind: SpecialKind, health: f32, at: Vec3) -> Self {
        let tier = match kind {
            SpecialKind::Napalm | SpecialKind::Sapper => SpeedTier::Walker,
            SpecialKind::Shrieker => SpeedTier::Sprinter,
        };
        Self {
            special: Some(kind),
            ..Self::new(health, tier, at)
        }
    }

    /// How fast it moves: by its Speed Tier, `CRAWLER_SPEED` once it's a Crawler,
    /// `HELLHOUND_SPEED` for a Hellhound, `NOVA_CRAWLER_SPEED` for a Nova Crawler, or
    /// `SAPPER_SPEED` for the Sapper.
    pub fn speed(&self) -> f32 {
        if self.is_sapper() {
            SAPPER_SPEED
        } else if self.hellhound {
            HELLHOUND_SPEED
        } else if self.nova {
            NOVA_CRAWLER_SPEED
        } else if self.crawler {
            CRAWLER_SPEED
        } else {
            self.tier.speed()
        }
    }

    /// Health left; at or below 0 once killed.
    pub fn health(&self) -> f32 {
        self.health
    }

    /// The health it spawned with.
    pub fn max_health(&self) -> f32 {
        self.max_health
    }

    /// Scales a fresh Zombie's health and max health by `factor` (the `ZombieHealth` stat).
    pub fn scale_health(&mut self, factor: f32) {
        self.health *= factor;
        self.max_health *= factor;
    }

    /// Switches to Sprinter, as the last Zombie of an early Round does. A Crawler never sprints,
    /// a Hellhound already runs at its own speed, a Napalm only ever walks, and the Sapper keeps
    /// its own pace.
    pub fn sprint(&mut self) {
        if !self.crawler && !matches!(self.special, Some(SpecialKind::Napalm | SpecialKind::Sapper)) {
            self.tier = SpeedTier::Sprinter;
        }
    }

    pub fn is_crawler(&self) -> bool {
        self.crawler
    }

    pub fn tier(&self) -> SpeedTier {
        self.tier
    }

    pub fn is_hellhound(&self) -> bool {
        self.hellhound
    }

    pub fn is_nova_crawler(&self) -> bool {
        self.nova
    }

    /// Which kind of Special Zombie it is, if it's a Napalm, a Shrieker or the Sapper.
    pub fn special_kind(&self) -> Option<SpecialKind> {
        self.special
    }

    /// A puppet Zombie that the Host says is a Napalm, a Shrieker or the Sapper is one here.
    pub fn become_special(&mut self, kind: SpecialKind) {
        self.special = Some(kind);
    }

    pub fn is_sapper(&self) -> bool {
        self.special == Some(SpecialKind::Sapper)
    }

    /// Whether it's a Special Zombie (a Hellhound, a Nova Crawler, a Napalm, a Shrieker or the
    /// Sapper) rather than a normal Zombie. A Crawler is a normal Zombie that lost its legs.
    pub fn is_special(&self) -> bool {
        self.hellhound || self.nova || self.special.is_some()
    }

    /// Whether an explosion can make a Crawler of it: only a normal Zombie that still has its legs.
    pub fn can_lose_legs(&self) -> bool {
        !self.crawler && !self.is_special()
    }

    /// From its centre down to its feet.
    pub(crate) fn half_height(&self) -> f32 {
        self.scale
            * if self.hellhound {
                HELLHOUND_RADIUS
            } else if self.nova {
                NOVA_CRAWLER_RADIUS
            } else if self.crawler {
                CRAWLER_RADIUS
            } else {
                body_half_height()
            }
    }

    /// Its attack: reach, wind-up, damage, and the cooldown after one lands. A hound's bites land
    /// `HELLHOUND_BITE_SECS` apart; a Nova Crawler's swipes hit for `NOVA_CRAWLER_SWIPE_DAMAGE`.
    fn attack_stats(&self) -> (f32, f32, f32, f32) {
        let (reach, wind_up, damage, cooldown) = self.unscaled_attack_stats();
        // A bigger body reaches past its own wider sides.
        (reach + ZOMBIE_RADIUS * (self.scale - 1.0), wind_up, damage, cooldown)
    }

    fn unscaled_attack_stats(&self) -> (f32, f32, f32, f32) {
        if self.hellhound {
            (
                HELLHOUND_REACH,
                HELLHOUND_BITE_WIND_UP_SECS,
                HELLHOUND_BITE_DAMAGE,
                HELLHOUND_BITE_SECS - HELLHOUND_BITE_WIND_UP_SECS,
            )
        } else if self.nova {
            (ZOMBIE_REACH, ZOMBIE_WIND_UP_SECS, NOVA_CRAWLER_SWIPE_DAMAGE, ZOMBIE_HIT_COOLDOWN_SECS)
        } else if self.special == Some(SpecialKind::Napalm) {
            (ZOMBIE_REACH, ZOMBIE_WIND_UP_SECS, NAPALM_SWIPE_DAMAGE, ZOMBIE_HIT_COOLDOWN_SECS)
        } else if self.is_sapper() {
            (SAPPER_SLAM_REACH, SAPPER_SLAM_WIND_UP_SECS, SAPPER_SLAM_DAMAGE, SAPPER_SLAM_COOLDOWN_SECS)
        } else {
            (ZOMBIE_REACH, ZOMBIE_WIND_UP_SECS, ZOMBIE_SWIPE_DAMAGE, ZOMBIE_HIT_COOLDOWN_SECS)
        }
    }
}

/// A Zombie pulled by a Monkey Bomb: it heads for this point instead of the Player.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Lured(pub Vec3);

/// A knocked-down Zombie, sliding to a stop and lying there until it gets up. It can't move
/// or attack meanwhile.
#[derive(Component)]
pub struct KnockedDown {
    elapsed: f32,
    push: Vec3,
    /// Which way it faced when it went down, so it gets up facing the same way.
    facing: Quat,
}

/// One of a Zombie's hit areas. Its parent is the Zombie.
#[derive(Component)]
pub struct HitArea(pub HitZone);

/// A Riser climbing out of the ground, or a Nova Crawler dropping from its Vent. It can be shot,
/// but can't move or attack until it's in place.
#[derive(Component)]
pub struct Rising {
    elapsed: f32,
    /// Its height when it spawned, and once in place.
    buried: f32,
    standing: f32,
    /// Seconds it takes to get there.
    secs: f32,
}

impl Rising {
    /// Its centre's height when it started, buried.
    pub fn buried(&self) -> f32 {
        self.buried
    }

    /// Moving its centre from height `from` to `to` over `secs`.
    pub fn new(from: f32, to: f32, secs: f32) -> Self {
        Self {
            elapsed: 0.0,
            buried: from,
            standing: to,
            secs,
        }
    }
}

/// A killed Zombie falling over before it disappears.
#[derive(Component, Default)]
pub struct Dying {
    elapsed: f32,
    /// Seconds of its fall it has sunk toward the floor through, counted in the physics step.
    sunk: f32,
    /// The attack that killed it, when it was killed rather than falling as a Turned ally.
    killed_by: Option<Attack>,
    /// The Player whose attack killed it.
    killer: Option<Entity>,
    /// A hit to the head killed it.
    headless: bool,
}

impl Dying {
    /// The attack that killed it.
    pub fn killed_by(&self) -> Option<Attack> {
        self.killed_by
    }

    /// The Player whose attack killed it, if one did.
    pub fn killer(&self) -> Option<Entity> {
        self.killer
    }

    /// Whether a hit to the head killed it.
    pub fn headless(&self) -> bool {
        self.headless
    }
}

#[derive(Resource)]
struct ZombieAssets {
    head: Handle<Mesh>,
    torso: Handle<Mesh>,
    arm: Handle<Mesh>,
    hand: Handle<Mesh>,
    leg: Handle<Mesh>,
    stump: Handle<Mesh>,
    eye: Handle<Mesh>,
    mouth: Handle<Mesh>,
    crawler_body: Handle<Mesh>,
    skin: Handle<StandardMaterial>,
    clothes: Handle<StandardMaterial>,
    trousers: Handle<StandardMaterial>,
    eyes: Handle<StandardMaterial>,
    mouth_mat: Handle<StandardMaterial>,
}

/// Zombies are interpolated, so their translation is only ever written in the physics step
/// (`FixedUpdate`). A write from `Update` is undone: whenever the next frame runs a physics step,
/// the interpolation first completes the easing it was doing and puts the Zombie back where the
/// last step left it (see `crate::players::player::Teleport`). Any other change to a Zombie's Transform
/// in `Update`, even a turn, counts as a teleport and throws the easing away, so it freezes on
/// frames without a step. Turns drawn every frame go in this set instead: right after the
/// interpolation has eased the translation, and before it notes that it has.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct FrameTurnSystems;

pub struct ZombiePlugin;

impl Plugin for ZombiePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<DamageZombie>()
            .add_message::<StunZombie>()
            .add_message::<ZombieHit>()
            .add_message::<ZombieKilled>()
            .add_message::<KillDamage>()
            .add_message::<ZombieCleanedUp>()
            .add_message::<KnockDown>()
            .add_message::<Gibbed>()
            .add_message::<TurnedAllyHit>()
            .add_message::<ZombieSpawned>()
            .add_message::<KillAllZombies>()
            .add_message::<HealZombie>()
            .add_systems(Startup, load_assets)
            .configure_sets(
                RunFixedMainLoop,
                FrameTurnSystems
                    .in_set(RunFixedMainLoopSystems::AfterFixedMainLoop)
                    .after(TransformEasingSystems::Ease)
                    .before(TransformEasingSystems::UpdateEasingTick),
            )
            .add_systems(
                FixedUpdate,
                (rise, wade.before(chase), chase, tumble, sink).in_set(HostSimulation).run_if(world_runs),
            )
            .add_systems(
                RunFixedMainLoop,
                fall_over.run_if(world_runs).in_set(FrameTurnSystems),
            )
            .add_systems(Update, (rig::animate.run_if(world_runs), rig::wear_outfits))
            .add_systems(
                Update,
                (
                    attack,
                    knock_down,
                    stun,
                    // First, so a Zombie killed in the same frame dies as the Crawler it became.
                    become_crawler.before(apply_damage),
                    kill_all.before(apply_damage),
                    apply_damage,
                    clean_up_stuck,
                    heal_zombies,
                    scale_spawn_health,
                    announce_spawns.after(scale_spawn_health),
                )
                    .in_set(HostSimulation)
                    .run_if(world_runs),
            );
    }
}

fn load_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Blocky, all boxes: see `rig`.
    let matte = |colour: Color| StandardMaterial {
        base_color: colour,
        perceptual_roughness: 0.95,
        ..default()
    };
    commands.insert_resource(ZombieAssets {
        head: meshes.add(Cuboid::from_length(rig::HEAD)),
        torso: meshes.add(Cuboid::from_size(rig::TORSO)),
        arm: meshes.add(Cuboid::from_size(rig::ARM)),
        hand: meshes.add(Cuboid::from_size(rig::HAND)),
        leg: meshes.add(Cuboid::from_size(rig::LEG)),
        stump: meshes.add(Cuboid::new(rig::LEG.x, rig::LEG.x, 0.16)),
        eye: meshes.add(Cuboid::new(0.08, 0.06, 0.02)),
        mouth: meshes.add(Cuboid::new(0.2, 0.05, 0.02)),
        // Lying along its facing (-Z), low on the floor, dragging itself forward.
        crawler_body: meshes.add(Mesh::from(Cuboid::from_size(rig::CRAWLER_BODY)).translated_by(Vec3::Y * -0.1)),
        skin: materials.add(matte(Color::srgb(0.42, 0.55, 0.36))),
        clothes: materials.add(matte(Color::srgb(0.2, 0.3, 0.34))),
        trousers: materials.add(matte(Color::srgb(0.2, 0.2, 0.3))),
        eyes: materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.6, 0.1),
            emissive: LinearRgba::rgb(8.0, 4.0, 0.5),
            ..default()
        }),
        mouth_mat: materials.add(matte(Color::srgb(0.12, 0.05, 0.05))),
    });
}

fn body_half_height() -> f32 {
    ZOMBIE_RADIUS + ZOMBIE_CAPSULE_LENGTH / 2.0
}

/// Makes a Zombie just spawned standing at `at` (not rising) `scale` times the usual size, its
/// feet still on the floor there. Its colliders and hit areas grow with it.
pub fn scale_body(zombie: &mut EntityCommands, at: Vec3, scale: f32) {
    if scale == 1.0 {
        return;
    }
    let centre = Vec3::new(at.x, at.y + body_half_height() * scale + 0.02, at.z);
    zombie.insert(Transform::from_translation(centre).with_scale(Vec3::splat(scale)));
}

/// How far a Riser's centre starts under its standing height, so the top of its head is below
/// the floor.
fn buried_depth() -> f32 {
    body_half_height() + rig::head_rest().y + ZOMBIE_HEAD_RADIUS + 0.05
}

/// The floor under a rising Zombie whose centre is at height `centre_y`, as it starts: where its
/// feet will be when it has stood up.
pub fn rising_floor(centre_y: f32) -> f32 {
    centre_y + buried_depth() - body_half_height() - 0.02
}

/// Spawns a Zombie standing on the floor at `at`, or, for a Riser, buried under it and about to
/// climb out over `rise` seconds.
pub fn spawn_zombie(
    commands: &mut Commands,
    assets: &ZombieAssetsParam,
    at: Vec3,
    health: f32,
    tier: SpeedTier,
    rise: Option<f32>,
) -> Entity {
    let assets = &assets.0;
    let half = body_half_height();
    let head_offset = rig::head_rest().y;
    // Start just above the floor; gravity settles it.
    let standing = at.y + half + 0.02;
    // A Riser starts with the top of its head below the floor.
    let buried = standing - buried_depth();
    let centre = Vec3::new(at.x, if rise.is_some() { buried } else { standing }, at.z);

    let mut zombie = commands.spawn_empty();
    let root = zombie.id();
    if let Some(secs) = rise {
        zombie.insert(Rising::new(buried, standing, secs));
    }
    let mut body = None;
    zombie
        .insert((
            InRun,
            Name::new("Zombie"),
            Zombie::new(health, tier, centre),
            Route::default(),
            // Its outfit, worn by the rig's boxes (see `rig::Outfit`).
            MeshMaterial3d(assets.clothes.clone()),
            Transform::from_translation(centre),
            Visibility::default(),
            RigidBody::Kinematic,
            CustomPositionIntegration,
            TranslationInterpolation,
            Collider::capsule(ZOMBIE_RADIUS, ZOMBIE_CAPSULE_LENGTH),
            CollisionLayers::new(GameLayer::Zombie, LayerMask::ALL),
        ))
        .with_children(|zombie| {
            let hit_layers = CollisionLayers::new(GameLayer::ZombieHitArea, LayerMask::ALL);
            // The head's hit area carries the head itself, and leans with the rig.
            let head = zombie
                .spawn((
                    HitArea(HitZone::Head),
                    Mesh3d(assets.head.clone()),
                    MeshMaterial3d(assets.skin.clone()),
                    Transform::from_xyz(0.0, head_offset, 0.0),
                    Collider::cuboid(rig::HEAD, rig::HEAD, rig::HEAD),
                    hit_layers,
                ))
                .with_children(|head| rig::face(head, assets, rig::HEAD))
                .id();
            // The neck leans with the rig too, so it stays under the head.
            let neck_bottom = head_offset - ZOMBIE_HEAD_RADIUS - ZOMBIE_NECK_SIZE.y;
            let neck = zombie
                .spawn((
                    HitArea(HitZone::Neck),
                    Transform::from_xyz(0.0, neck_bottom + ZOMBIE_NECK_SIZE.y / 2.0, 0.0),
                    box_collider(ZOMBIE_NECK_SIZE),
                    hit_layers,
                ))
                .id();
            body = Some(rig::spawn_standing(zombie, assets, root, head, neck));
            // Arms reaching forward (-Z is forward), where the rig's arms shamble.
            for x in [-ZOMBIE_RADIUS - 0.05, ZOMBIE_RADIUS + 0.05] {
                zombie.spawn((
                    HitArea(HitZone::Limb),
                    Transform::from_xyz(x, half * 0.55, -0.3),
                    box_collider(ZOMBIE_ARM_SIZE),
                    hit_layers,
                ));
            }

            // Invisible hit areas inside the body capsule, stacked from the feet up: legs, then
            // torso, up to the neck under the head (above).
            let torso_bottom = -half + ZOMBIE_LEG_SIZE.y;
            for x in [-ZOMBIE_LEG_OFFSET, ZOMBIE_LEG_OFFSET] {
                zombie.spawn((
                    HitArea(HitZone::Limb),
                    Transform::from_xyz(x, -half + ZOMBIE_LEG_SIZE.y / 2.0, 0.0),
                    box_collider(ZOMBIE_LEG_SIZE),
                    hit_layers,
                ));
            }
            zombie.spawn((
                HitArea(HitZone::Torso),
                Transform::from_xyz(0.0, (torso_bottom + neck_bottom) / 2.0, 0.0),
                Collider::cuboid(ZOMBIE_TORSO_WIDTH, neck_bottom - torso_bottom, ZOMBIE_TORSO_DEPTH),
                hit_layers,
            ));
        });
    if let Some(body) = body {
        zombie.insert(body);
    }
    root
}

/// A Crawler's parts, around a body lying low along its facing: head and neck at the front, arms
/// dragging it along, a torso, and no legs. Returns its rig.
fn spawn_crawler_parts(crawler: &mut ChildSpawnerCommands, assets: &ZombieAssets, root: Entity) -> rig::Rig {
    let hit_layers = CollisionLayers::new(GameLayer::ZombieHitArea, LayerMask::ALL);
    let front = -(CRAWLER_BODY_LENGTH / 2.0 + CRAWLER_RADIUS);
    crawler
        .spawn((
            HitArea(HitZone::Head),
            Mesh3d(assets.head.clone()),
            MeshMaterial3d(assets.skin.clone()),
            Transform::from_xyz(0.0, CRAWLER_RADIUS - ZOMBIE_HEAD_RADIUS, front),
            Collider::cuboid(rig::HEAD, rig::HEAD, rig::HEAD),
            hit_layers,
        ))
        .with_children(|head| rig::face(head, assets, rig::HEAD));
    crawler.spawn((
        HitArea(HitZone::Neck),
        Transform::from_xyz(0.0, 0.0, front + ZOMBIE_HEAD_RADIUS + ZOMBIE_NECK_SIZE.z / 2.0),
        box_collider(ZOMBIE_NECK_SIZE),
        hit_layers,
    ));
    crawler.spawn((
        HitArea(HitZone::Torso),
        Transform::from_xyz(0.0, 0.0, ZOMBIE_NECK_SIZE.z / 2.0),
        Collider::cuboid(CRAWLER_RADIUS * 2.0, CRAWLER_RADIUS * 2.0, CRAWLER_BODY_LENGTH + CRAWLER_RADIUS),
        hit_layers,
    ));
    // Arms out in front on the floor, where the rig's arms drag it along.
    for x in [-CRAWLER_RADIUS - 0.05, CRAWLER_RADIUS + 0.05] {
        crawler.spawn((
            HitArea(HitZone::Limb),
            Transform::from_xyz(x, ZOMBIE_ARM_SIZE.y / 2.0 - CRAWLER_RADIUS, front),
            box_collider(ZOMBIE_ARM_SIZE),
            hit_layers,
        ));
    }
    rig::spawn_crawling(crawler, assets, root)
}

fn box_collider(size: Vec3) -> Collider {
    Collider::cuboid(size.x, size.y, size.z)
}

/// Lets other modules spawn Zombies without knowing about the asset handles.
#[derive(bevy::ecs::system::SystemParam)]
pub struct ZombieAssetsParam<'w>(Res<'w, ZombieAssets>);

fn horizontal(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

/// Separation steering: a push away from Zombies closer than `ZOMBIE_SEPARATION_DIST`, stronger
/// the closer they are. Zombies never spread out further than this, so Trains still form.
pub fn separation(me: Vec3, others: impl IntoIterator<Item = Vec3>) -> Vec3 {
    let mut push = Vec3::ZERO;
    for other in others {
        let away = horizontal(me - other);
        let d = away.length();
        if d > 1e-4 && d < ZOMBIE_SEPARATION_DIST {
            push += away / d * (1.0 - d / ZOMBIE_SEPARATION_DIST);
        }
    }
    push
}

/// How far a Zombie-sized body, its middle `half` above its feet, gets walking straight from
/// `from` towards `to` (its middle at each), measured along the floor, before the Map stops it:
/// something in the way, or a Ledge to climb. `None` if nothing does.
fn walk_stop(spatial: &SpatialQuery, probe: &Collider, from: Vec3, to: Vec3, half: f32) -> Option<f32> {
    let delta = to - from;
    let Ok(dir) = Dir3::new(delta) else {
        return None;
    };
    let flat = horizontal(delta).length();
    let map = SpatialQueryFilter::from_mask(GameLayer::World);
    let body = spatial
        .cast_shape(probe, from, Quat::IDENTITY, dir, &ShapeCastConfig::from_max_distance(delta.length()), &map)
        .map(|hit| hit.distance / delta.length() * flat);
    // Straight up or down, there's no floor to cross.
    if flat < 1e-3 {
        return body;
    }
    // The floor only matters as far as the body gets. Its side meets a Ledge's face a body's
    // width short of it.
    let reach = body.map_or(1.0, |stop| stop / flat);
    let feet = from - Vec3::Y * half;
    let ledge = floor_ledge(spatial, feet, feet + (delta * reach), &map).map(|face| (face - ZOMBIE_RADIUS).max(0.0));
    ledge.or(body)
}

/// Whether a Zombie-sized body can walk straight from `from` to `to` (see `walk_stop`) without
/// the Map stopping it.
fn clear_walk(spatial: &SpatialQuery, probe: &Collider, from: Vec3, to: Vec3, half: f32) -> bool {
    walk_stop(spatial, probe, from, to, half).is_none()
}

/// The path nodes a Zombie-sized body, its middle at `centre` and `half` above its feet, follows
/// to its target standing at `target_feet`: none where it can walk straight there, or where what
/// stops it (a table, the edge of a Ledge, a crate the Player is up on) leaves it within this
/// share of `reach` to swipe across. Otherwise its route runs from one of the nearest nodes it can
/// walk to, to one of the nearest its target can be walked to from, whichever pair makes the
/// shortest walk all told: nodes behind a crate or over a Ledge are no use to either. No route at all (the drop is one-way) leaves it steering straight at its
/// target until the stuck rule replaces it.
pub(crate) fn plan_route(
    graph: &NavGraph,
    spatial: &SpatialQuery,
    centre: Vec3,
    half: f32,
    target_feet: Vec3,
    reach: f32,
    state: NavState,
) -> Vec<Vec3> {
    let probe = Collider::sphere(ZOMBIE_RADIUS * 0.8);
    let feet = centre - Vec3::Y * half;
    let target = target_feet + Vec3::Y * half;
    let walk = |from: Vec3, to: Vec3| clear_walk(spatial, &probe, from, to, half);
    let level = (target_feet.y - feet.y).abs() <= ZOMBIE_DIRECT_MAX_RISE;
    let closes_in = |stop: f32| horizontal(target - centre).length() - stop <= reach * ZOMBIE_CLOSE_IN_SHARE;
    if level && walk_stop(spatial, &probe, centre, target, half).is_none_or(closes_in) {
        return Vec::new();
    }
    let mut from = graph.near_where(feet, |node| walk(centre, node.at + Vec3::Y * half));
    if from.is_empty() {
        from.push((graph.nearest(feet), 0.0));
    }
    let mut to = graph.near_where(target_feet, |node| walk(node.at + Vec3::Y * half, target));
    if to.is_empty() {
        to.push((graph.nearest(target_feet), 0.0));
    }
    let path = graph.plan_between(&from, &to, state).unwrap_or_default();
    let mut waypoints: Vec<Vec3> = path.into_iter().map(|i| graph.nodes[i].at).collect();
    // Skip the first node while the second is a clear walk on the same floor, or the body is
    // already on the edge between them (a ramp changes floor on the way), so it never doubles
    // back to a node behind it. More than one can be behind it: through a window it is past the
    // node inside the sill, then past the one after that.
    while let [first, second, ..] = waypoints[..]
        && (already_past(first, second, feet)
            || ((second.y - feet.y).abs() < 0.5 && (first.y - feet.y).abs() < 0.5 && walk(centre, second + Vec3::Y * half)))
    {
        waypoints.remove(0);
    }
    waypoints
}

/// How many of `waypoints` a Zombie at `feet` can skip: those before the furthest of the next
/// `ZOMBIE_LOOK_AHEAD_NODES` that `walk` reaches in a clear line, all on its own floor, so ramps
/// and drops are never cut. Never the last.
fn look_ahead(waypoints: &[Vec3], feet: Vec3, walk: impl Fn(Vec3) -> bool) -> usize {
    let near = waypoints.iter().take(ZOMBIE_LOOK_AHEAD_NODES + 1).take_while(|w| (w.y - feet.y).abs() < 0.5).count();
    (1..near).rev().find(|&i| walk(waypoints[i])).unwrap_or(0)
}

/// A steady number in -1..=1 that is the same for `entity` every time, and differs by `salt`.
fn personal(entity: Entity, salt: u64) -> f32 {
    let bits = (entity.to_bits() ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    ((bits >> 40) & 0xFFFF) as f32 / 32767.5 - 1.0
}

/// Which way a Zombie walks along the line to the Player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Heading {
    Toward,
    Away,
    Stay,
}

/// Where a Zombie with `reach` heads, `distance` (horizontal) from the Player. It plants its feet
/// while swinging, so the Player can back out of reach. While the Player is in Last Stand, Zombies
/// walk away until they're about `LAST_STAND_ZOMBIE_RETREAT_DISTANCE` off.
pub fn heading(distance: f32, reach: f32, swinging: bool, last_stand: bool) -> Heading {
    if last_stand {
        if distance < LAST_STAND_ZOMBIE_RETREAT_DISTANCE { Heading::Away } else { Heading::Stay }
    } else if swinging || distance <= reach * 0.8 {
        Heading::Stay
    } else {
        Heading::Toward
    }
}

/// How far outside the heights of an edge's two nodes a Zombie's feet may be and still be on it.
const ALREADY_PAST_HEIGHT: f32 = 0.5;

/// Whether a Zombie at `feet`, planning from its nearest node `first` to the next one `second`, is
/// already on the edge between them and past `first`. Halfway up a ramp the nearest node is still
/// the one behind, and without this the Zombie keeps turning back for it and never climbs. No cast
/// is needed: the Map's tests check every edge is walkable, and a straight cast clips a ramp.
/// Measured across the floor: off a drop the edge plunges, so a Zombie walking out to the Ledge
/// would only get further from it, and turn back for the node behind it at the brink. Its feet
/// must also lie between the two nodes' heights, so a Zombie on another floor under or over the
/// edge is never on it.
fn already_past(first: Vec3, second: Vec3, feet: Vec3) -> bool {
    let edge = horizontal(second - first);
    if edge.length_squared() < 1e-4 {
        return false;
    }
    let (low, high) = (first.y.min(second.y) - ALREADY_PAST_HEIGHT, first.y.max(second.y) + ALREADY_PAST_HEIGHT);
    let along = horizontal(feet - first).dot(edge) / edge.length_squared();
    (low..=high).contains(&feet.y) && along > 0.0 && horizontal(feet - first - edge * along.min(1.0)).length() < ZOMBIE_WAYPOINT_REACH
}

/// Zombies find a Player (or the Monkey Bomb pulling them) anywhere on the Map: every so often they
/// pick the nearest Player by path who is up, and re-plan a route over the path-node graph. They
/// steer straight at the Player when there's a clear walk or they share a node area, and move
/// under gravity with grounded move-and-slide.
fn chase(
    mut zombies: Query<
        (
            Entity,
            &mut Zombie,
            &mut Route,
            &mut Transform,
            &Collider,
            Option<&Lured>,
            Option<&Seeking>,
            Option<&ZombieMoveFactor>,
        ),
        (
            Without<Dying>,
            Without<Rising>,
            Without<KnockedDown>,
            Without<Stunned>,
            Without<Entering>,
            Without<Telegraph>,
        ),
    >,
    players: Players,
    graph: Res<NavGraph>,
    doors: Query<&Door>,
    pieces: Query<&crate::maps::shifting::PieceState>,
    barriers: Res<Barriers>,
    move_and_slide: MoveAndSlide,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    let open_doors: Vec<_> = doors.iter().filter(|door| door.open).filter_map(|door| door.route()).collect();
    let rests = crate::maps::shifting::standing_rests(pieces.iter().map(|state| (state.id(), state.phase())), graph.map.layout().shifting.len());
    let barrier_open = |b| barriers.is_open(b);
    let nav = NavState::new(&open_doors, &barrier_open).with_rests(&rests);
    let filter_mask = [GameLayer::World, GameLayer::Zombie, GameLayer::Player];
    let positions: Vec<_> = zombies.iter().map(|(entity, _, _, transform, ..)| (entity, transform.translation)).collect();

    for (entity, mut zombie, mut route, mut transform, collider, lured, seeking, move_factor) in &mut zombies {
        let half = zombie.half_height();
        let feet = transform.translation - Vec3::Y * half;

        // Picks the nearest Player who is up, again every so often so a Train can pass between
        // Players, and at once when its Player goes down. With none up (a solo Last Stand) it
        // stays with the nearest, and leaves them alone.
        route.retarget_in -= dt;
        let held = route.target.and_then(|target| players.get(target));
        if route.retarget_in <= 0.0 || held.is_none_or(|player| !player.up) {
            route.retarget_in = ZOMBIE_RETARGET_SECS;
            let picked = nearest_up_player(&graph, feet, players.iter(), nav)
                .or_else(|| players.nearest(feet));
            if let Some(picked) = picked
                && route.target != Some(picked.entity)
            {
                route.target = Some(picked.entity);
                route.repath_in = 0.0;
            }
        }
        let Some(player) = route.target.and_then(|target| players.get(target)) else {
            continue;
        };
        let modifiers = players.modifiers(player.entity).map_or(&[][..], |modifiers| &modifiers.0[..]);

        // A point of its own beats everything; then a Monkey Bomb's pull, even over a Player in
        // Last Stand.
        let goal_feet = seeking.map(|seek| seek.0).or(lured.map(|lure| lure.0));
        let target_feet = goal_feet.unwrap_or(player.stand);
        let last_stand = !player.up && goal_feet.is_none();
        let rise = target_feet.y - feet.y;

        route.repath_in -= dt;
        if route.repath_in <= 0.0 {
            route.repath_in = repath_interval(feet.distance(target_feet));
            route.waypoints = plan_route(
                &graph,
                &move_and_slide.spatial_query,
                transform.translation,
                half,
                target_feet,
                zombie.attack_stats().0,
                nav,
            );
        }

        // Looks ahead, every so often, for a node further along that it can walk straight to, and
        // goes on from there: it turns for the next doorway as soon as it can see it.
        let probe = Collider::sphere(ZOMBIE_RADIUS * 0.8);
        let walk = |from: Vec3, to: Vec3| clear_walk(&move_and_slide.spatial_query, &probe, from, to, half);
        route.look_in -= dt;
        if route.look_in <= 0.0 {
            route.look_in = ZOMBIE_LOOK_AHEAD_SECS * (1.0 + 0.5 * personal(entity, 1).abs());
            route.bend_for = None;
            let skip = look_ahead(&route.waypoints, feet, |to| walk(transform.translation, to + Vec3::Y * half));
            route.waypoints.drain(..skip);
        }

        // Its own sideways shift off the line through the next node, where there's room.
        if route.waypoints.first().copied() != route.side_for {
            route.side_for = route.waypoints.first().copied();
            route.side = Vec3::ZERO;
            if let [next, after, ..] = route.waypoints[..]
                && (after.y - next.y).abs() < 0.5
                && (next.y - feet.y).abs() < 0.5
            {
                let line = horizontal(after - next).normalize_or_zero();
                let across = Vec3::new(-line.z, 0.0, line.x) * personal(entity, 2) * ZOMBIE_SIDE_OFFSET;
                let lift = Vec3::Y * half;
                route.side = [1.0, 0.5]
                    .into_iter()
                    .map(|share| across * share)
                    .find(|&shift| {
                        walk(transform.translation, next + shift + lift) && walk(next + shift + lift, after + lift)
                    })
                    .unwrap_or(Vec3::ZERO);
            }
        }

        // Reached the next node: on to the one after. Where the route turns, sooner, and
        // steering bends towards the node after over the last stretch.
        let mut aim = route.waypoints.first().map(|&next| next + route.side);
        if let Some(&next) = route.waypoints.first()
            && (next.y - feet.y).abs() < 1.5
        {
            let at = next + route.side;
            let distance = horizontal(at - feet).length();
            let outer = ZOMBIE_WAYPOINT_REACH + ZOMBIE_CORNER_REACH;
            let mut reach = ZOMBIE_WAYPOINT_REACH;
            if let Some(&after) = route.waypoints.get(1)
                && distance < outer
                && (after.y - next.y).abs() < 0.5
                && (next.y - feet.y).abs() < 0.5
            {
                if route.bend_for != Some(next) {
                    route.bend_for = Some(next);
                    route.bend_clear = walk(transform.translation, after + Vec3::Y * half);
                }
                if route.bend_clear {
                    let turn = horizontal(at - feet).angle_between(horizontal(after - at)) / std::f32::consts::PI;
                    reach += ZOMBIE_CORNER_REACH * turn;
                    let bend = 0.5 * turn * (1.0 - (distance / outer).min(1.0));
                    aim = Some(at.lerp(after, bend));
                }
            }
            if distance < reach {
                route.waypoints.remove(0);
                aim = route.waypoints.first().map(|&next| next + route.side);
            }
        }
        let goal = aim.unwrap_or(target_feet);

        let to_player = horizontal(target_feet - feet);
        // Same floor only: a Zombie under the Balcony never stops to swing at the Player on it.
        let reach_distance = if rise.abs() < 1.5 { to_player.length() } else { f32::INFINITY };
        let head = heading(reach_distance, zombie.attack_stats().0, zombie.wind_up.is_some(), last_stand);
        let mut heading = horizontal(goal - feet).normalize_or_zero();
        let neighbours = positions.iter().filter(|(other, _)| *other != entity).map(|(_, at)| *at);
        heading = (heading + separation(transform.translation, neighbours) * ZOMBIE_SEPARATION_WEIGHT).normalize_or_zero();

        // A Player in Last Stand is left alone: Zombies walk straight away until they're well off.
        let planted = head == Heading::Stay;
        if head == Heading::Away {
            heading = (-to_player).normalize_or_zero();
        }
        let facing = if planted || (route.waypoints.is_empty() && head == Heading::Toward) {
            to_player
        } else {
            heading
        };
        if facing.length_squared() > 1e-4 {
            transform.rotation = Quat::from_rotation_y(f32::atan2(-facing.x, -facing.z));
        }
        let speed = moved_speed(resolve(Stat::ZombieSpeed, zombie.speed(), modifiers), move_factor);
        let stride = if planted { Vec3::ZERO } else { heading * speed };
        let velocity = stride + Vec3::Y * (zombie.fall_speed - GRAVITY * dt);

        let mut grounded = false;
        let out = move_and_slide.move_and_slide(
            collider,
            transform.translation,
            transform.rotation,
            velocity,
            time.delta(),
            &MoveAndSlideConfig::default(),
            &SpatialQueryFilter::from_mask(filter_mask).with_excluded_entities([entity]),
            |hit| {
                if hit.normal.y > 0.7 {
                    grounded = true;
                }
                MoveAndSlideHitResponse::Accept
            },
        );
        transform.translation = out.position;
        // Standing on the floor (or a slope) cancels any upward kick, so Zombies never hop.
        zombie.fall_speed = if grounded {
            out.projected_velocity.y.min(0.0)
        } else {
            out.projected_velocity.y
        };
    }
}

/// Risers climb out of the ground over their rise time (`ZOMBIE_RISE_SECS`, or a special's own),
/// and Nova Crawlers drop from their Vents. In the physics step, so the chase starts from where
/// they end up.
fn rise(mut commands: Commands, mut risers: Query<(Entity, &mut Rising, &mut Transform), Without<Dying>>, time: Res<Time>) {
    for (entity, mut rising, mut transform) in &mut risers {
        rising.elapsed += time.delta_secs();
        let t = (rising.elapsed / rising.secs).min(1.0);
        transform.translation.y = rising.buried.lerp(rising.standing, t);
        if t >= 1.0 {
            commands.entity(entity).remove::<Rising>();
        }
    }
}

/// A Zombie that's stuck, or lost far along its route from the Player, is quietly removed and sent back into the
/// Horde, so a Round never stalls. A Zombie outside a window is waiting on its Planks, not
/// stuck, so its clock only starts once it's in. A Crawler can't tear them, though, so for it
/// waiting at a planked Barrier too long is stuck. A Zombie Out of Bounds goes the same way at
/// once. A Zombie that carries over between Rounds is never cleaned up here, and nor is a
/// Hellhound Out of Bounds, nor a requested Zombie: their own modules move them instead. A Surge Zombie, which also
/// carries over, is cleaned up here but goes back into no Horde.
fn clean_up_stuck(
    mut commands: Commands,
    mut zombies: Query<
        (Entity, &mut Zombie, &Transform, Option<&Entering>, Has<Surge>, Option<&Route>),
        (Without<Dying>, Without<Rising>, Without<Telegraph>, Or<(Without<CarriesOver>, With<Surge>)>, Without<Requested>),
    >,
    players: Players,
    barriers: Res<Barriers>,
    map: Res<MapId>,
    tide: Option<Res<crate::maps::tide::Tide>>,
    mut cleaned: MessageWriter<ZombieCleanedUp>,
    time: Res<Time>,
) {
    let tide = tide.map_or_else(Default::default, |tide| tide.level());
    let dt = time.delta_secs();
    for (entity, mut zombie, transform, entering, surge, route) in &mut zombies {
        let gone = if map.layout().out_of_bounds(tide, transform.translation) {
            !zombie.hellhound
        } else if let Some(entering) = entering {
            zombie.stuck = StuckWatch::new(transform.translation);
            let planked = zombie.crawler && entering.at_barrier() && barriers.planks(entering.barrier) > 0;
            let stuck;
            (zombie.planked_wait, stuck) = planked_wait(zombie.planked_wait, planked, dt);
            stuck
        } else if let Some(player) = players.target_of(route.and_then(|route| route.target), transform.translation) {
            let waypoints = route.map_or(&[][..], |route| &route.waypoints[..]);
            let remaining = route_remaining(transform.translation, waypoints, player.centre);
            let speed = zombie.speed();
            zombie.stuck.update_on_route(transform.translation, player.centre, remaining, speed, dt)
        } else {
            false
        };
        if gone {
            commands.entity(entity).despawn();
            if !surge {
                cleaned.write(ZombieCleanedUp);
            }
        }
    }
}

/// Whether the Sapper at `from`, facing `forward`, can slam the Player at `to`: within
/// `SAPPER_SLAM_REACH` along the floor and inside the arc in front of it.
pub fn slam_reaches(from: Vec3, forward: Vec3, to: Vec3) -> bool {
    let flat = horizontal(to - from);
    let half_arc = (SAPPER_SLAM_ARC_DEGREES / 2.0).to_radians();
    flat.length() <= SAPPER_SLAM_REACH && (flat.length() < 1e-3 || horizontal(forward).angle_between(flat) <= half_arc)
}

/// Where Zombie-family melee is sent: swipes, reach-through swipes, bites and the Sapper's slam.
/// Each one judges the Player's Hit Grace, so a hit inside it is never sent, and one that lands
/// opens it. Explosions, Traps and fire write `PlayerHit` themselves and pass Hit Grace by.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct ZombieMelee<'w, 's> {
    graces: Query<'w, 's, &'static mut HitGrace>,
    hits: MessageWriter<'w, PlayerHit>,
    clock: Res<'w, Time<Virtual>>,
}

impl ZombieMelee<'_, '_> {
    /// Strikes `player` for `damage` from `from`, unless their Hit Grace blocks it. A blocked
    /// swing is spent all the same: the caller starts its cooldown either way, so a pile-up can't
    /// line its next swings up with the end of Hit Grace.
    pub(crate) fn strike(&mut self, player: Entity, damage: f32, from: Vec3) {
        let now = self.clock.elapsed();
        if self.graces.get_mut(player).map_or(true, |mut grace| grace.try_land(now)) {
            self.hits.write(PlayerHit {
                player,
                damage,
                from: Some(from),
            });
        }
    }
}

/// Zombies inside swipe the Player in reach, Hellhounds bite, and the Sapper slams the Player in
/// the arc in front of it, all only with a clear line to the Player: a wall or a closed Door
/// stops a swipe and its wind-up. Those outside a window reach through it instead (see the
/// Barriers). With no Player in reach, a Zombie swings the same way at a Turned ally in reach
/// (ADR 0009): the swing lands as `TurnedAllyHit` and hurts nothing.
fn attack(
    mut zombies: Query<
        (Entity, &mut Zombie, &Transform, Option<&Route>),
        (
            Without<Dying>,
            Without<Rising>,
            Without<KnockedDown>,
            Without<Stunned>,
            Without<Entering>,
            Without<Telegraph>,
        ),
    >,
    players: Players,
    allies: Query<(Entity, &Transform), (With<TurnedAlly>, Without<Zombie>)>,
    spatial: SpatialQuery,
    mut melee: ZombieMelee,
    mut ally_hits: MessageWriter<TurnedAllyHit>,
    mut cues: MessageWriter<SpecialCue>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    for (entity, mut zombie, transform, route) in &mut zombies {
        let Some(player) = players.target_of(route.and_then(|route| route.target), transform.translation) else {
            continue;
        };
        // Zombies leave a Player in Last Stand alone.
        if !player.up {
            zombie.wind_up = None;
            continue;
        }
        let (reach, wind_up, damage, cooldown) = zombie.attack_stats();
        // Same floor only: a Zombie under the Balcony can't swipe the Player standing on it.
        // The same-floor check runs centre to centre (both transforms are capsule centres), while
        // the sight ray aims at the Player's feet + `SWIPE_CHEST_HEIGHT` — standing chest height —
        // whatever their Stance, so knee-high cover never shields a Player from a swipe, but a
        // wall does.
        let reaches = |centre: Vec3, aim: Vec3| {
            horizontal(centre - transform.translation).length() <= reach
                && (centre.y - transform.translation.y).abs() < 1.5
                && (!zombie.is_sapper() || slam_reaches(transform.translation, *transform.forward(), centre))
                && in_sight(&spatial, transform.translation, aim)
        };
        let in_reach = reaches(player.centre, player.feet + Vec3::Y * SWIPE_CHEST_HEIGHT);
        // The Player comes first; otherwise the nearest Turned ally in reach takes the swing.
        let ally = (!in_reach)
            .then(|| {
                allies
                    .iter()
                    .filter(|(_, ally)| reaches(ally.translation, ally.translation))
                    .min_by(|(_, a), (_, b)| {
                        a.translation.distance(transform.translation).total_cmp(&b.translation.distance(transform.translation))
                    })
                    .map(|(ally, _)| ally)
            })
            .flatten();
        let at = Some(transform.translation);
        zombie.attack_cooldown = (zombie.attack_cooldown - dt).max(0.0);

        match zombie.wind_up {
            Some(left) if left - dt <= 0.0 => {
                zombie.wind_up = None;
                // The slam comes down whether or not the Player is still there.
                if zombie.is_sapper() {
                    cues.write(SpecialCue { cue: Cue::SapperSlam, at });
                    zombie.attack_cooldown = cooldown;
                }
                // A Zombie that lands a swipe is ignored for a moment; other Zombies can still hit,
                // once the Player's Hit Grace is over.
                if in_reach {
                    melee.strike(player.entity, damage, transform.translation);
                    // The Sapper's slam has its own sound, above.
                    if !zombie.is_sapper() {
                        cues.write(SpecialCue { cue: swing_cue(&zombie), at });
                    }
                    zombie.attack_cooldown = cooldown;
                } else if let Some(ally) = ally {
                    ally_hits.write(TurnedAllyHit { ally, by: entity });
                    if !zombie.is_sapper() {
                        cues.write(SpecialCue { cue: swing_cue(&zombie), at });
                    }
                    zombie.attack_cooldown = cooldown;
                }
            }
            Some(left) => zombie.wind_up = Some(left - dt),
            None if (in_reach || ally.is_some()) && zombie.attack_cooldown <= 0.0 => {
                zombie.wind_up = Some(wind_up);
                if zombie.is_sapper() {
                    cues.write(SpecialCue { cue: Cue::SapperWindup, at });
                }
            }
            None => {}
        }
    }
}

/// The sound of a landed swipe or bite.
fn swing_cue(zombie: &Zombie) -> Cue {
    if zombie.hellhound {
        Cue::HoundBite
    } else if zombie.nova {
        Cue::NovaAttack
    } else {
        Cue::ZombieAttack
    }
}

/// A knock-down drops any swing in progress. A Riser still climbing out, or a hound still in its
/// lightning, can't be knocked down. A Heavyweight keeps its feet: the Thundergun stuns it instead.
fn knock_down(
    mut commands: Commands,
    mut requests: MessageReader<KnockDown>,
    mut zombies: Query<
        (&mut Zombie, &Transform, Has<Heavyweight>),
        (Without<Dying>, Without<Rising>, Without<KnockedDown>, Without<Telegraph>),
    >,
    mut stuns: MessageWriter<StunZombie>,
) {
    for request in requests.read() {
        let Ok((mut zombie, transform, heavyweight)) = zombies.get_mut(request.zombie) else {
            continue;
        };
        if heavyweight {
            stuns.write(StunZombie {
                zombie: request.zombie,
                secs: SAPPER_THUNDERGUN_STUN_SECS,
            });
            continue;
        }
        zombie.wind_up = None;
        commands.entity(request.zombie).insert(KnockedDown {
            elapsed: 0.0,
            push: request.push,
            facing: transform.rotation,
        });
    }
}

/// A stun drops any swing in progress and holds the Zombie still until it wears off.
fn stun(
    mut commands: Commands,
    mut requests: MessageReader<StunZombie>,
    mut zombies: Query<(Entity, &mut Zombie, Option<&mut Stunned>), Without<Dying>>,
    time: Res<Time>,
) {
    for (entity, _, stunned) in &mut zombies {
        if let Some(mut stunned) = stunned {
            stunned.left -= time.delta_secs();
            if stunned.left <= 0.0 {
                commands.entity(entity).remove::<Stunned>();
            }
        }
    }
    for request in requests.read() {
        let Ok((_, mut zombie, stunned)) = zombies.get_mut(request.zombie) else {
            continue;
        };
        zombie.wind_up = None;
        match stunned {
            Some(mut stunned) => stunned.left = stunned.left.max(request.secs),
            None => {
                commands.entity(request.zombie).insert(Stunned { left: request.secs });
            }
        }
    }
}

/// Knocked-down Zombies slide to a stop, tipped over on their backs, then stand back up.
fn tumble(
    mut commands: Commands,
    mut zombies: Query<(Entity, &mut Zombie, &mut KnockedDown, &mut Transform, &Collider), Without<Dying>>,
    move_and_slide: MoveAndSlide,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    let filter_mask = [GameLayer::World, GameLayer::Zombie, GameLayer::Player];
    for (entity, mut zombie, mut down, mut transform, collider) in &mut zombies {
        down.elapsed += dt;
        let t = (down.elapsed / THUNDERGUN_KNOCKDOWN_SECS).min(1.0);
        // The fling dies away over the first half; it lies still for the rest.
        let push = down.push * (1.0 - 2.0 * t).max(0.0);
        let velocity = push + Vec3::Y * (zombie.fall_speed - GRAVITY * dt);
        let out = move_and_slide.move_and_slide(
            collider,
            transform.translation,
            down.facing,
            velocity,
            time.delta(),
            &MoveAndSlideConfig::default(),
            &SpatialQueryFilter::from_mask(filter_mask).with_excluded_entities([entity]),
            |_| MoveAndSlideHitResponse::Accept,
        );
        transform.translation = out.position;
        zombie.fall_speed = out.projected_velocity.y.min(0.0);
        // Over quickly, up again at the end.
        let tilt = (4.0 * t).min(4.0 * (1.0 - t)).min(1.0) * 1.3;
        transform.rotation = down.facing * Quat::from_rotation_x(tilt);
        if t >= 1.0 {
            transform.rotation = down.facing;
            commands.entity(entity).remove::<KnockedDown>();
        }
    }
}

/// A Zombie whose legs were blown off drops to the floor as a Crawler: a low, short body the
/// Player can jump over, with no leg hit areas. A Riser still climbing out keeps its legs.
fn become_crawler(
    mut commands: Commands,
    mut gibbed: MessageReader<Gibbed>,
    mut zombies: Query<(&mut Zombie, &mut Transform, &mut TranslationEasingState), (Without<Dying>, Without<Rising>)>,
    assets: Res<ZombieAssets>,
) {
    for &Gibbed { zombie: entity } in gibbed.read() {
        let Ok((mut zombie, mut transform, mut easing)) = zombies.get_mut(entity) else {
            continue;
        };
        if !zombie.can_lose_legs() {
            continue;
        }
        zombie.crawler = true;
        zombie.wind_up = None;
        // Dropped at once, before the next physics step, so the easing under way drops with it:
        // otherwise the step would complete it and put the body back up.
        let drop = Vec3::Y * (body_half_height() - CRAWLER_RADIUS);
        transform.translation -= drop;
        easing.start = easing.start.map(|start| start - drop);
        easing.end = easing.end.map(|end| end - drop);
        give_crawler_body(&mut commands, &assets, entity);
    }
}

/// Swaps a Zombie's standing body for a Crawler's, hit areas and all.
fn give_crawler_body(commands: &mut Commands, assets: &ZombieAssets, entity: Entity) {
    let mut body = None;
    commands
        .entity(entity)
        .insert((Mesh3d(assets.crawler_body.clone()), Collider::sphere(CRAWLER_RADIUS)))
        .queue(skin::despawn_body)
        .with_children(|crawler| body = Some(spawn_crawler_parts(crawler, assets, entity)));
    if let Some(body) = body {
        commands.entity(entity).insert(body);
    }
}

/// Spawns the body of a Zombie the Host has, on a client: standing, or a Crawler, its centre at
/// `centre` and facing `yaw`. It has no Health to speak of and nothing moves it.
pub fn spawn_puppet(
    commands: &mut Commands,
    assets: &ZombieAssetsParam,
    centre: Vec3,
    yaw: f32,
    crawler: bool,
) -> Entity {
    let feet = centre - Vec3::Y * (body_half_height() + 0.02);
    let entity = spawn_zombie(commands, assets, feet, 1.0, SpeedTier::Walker, None);
    commands
        .entity(entity)
        .insert(Transform::from_translation(centre).with_rotation(Quat::from_rotation_y(yaw)));
    if crawler {
        make_puppet_crawler(commands, assets, entity);
    }
    entity
}

/// A puppet Zombie that has lost its legs on the Host loses them here.
pub fn make_puppet_crawler(commands: &mut Commands, assets: &ZombieAssetsParam, entity: Entity) {
    commands.entity(entity).entry::<Zombie>().and_modify(|mut zombie| {
        zombie.crawler = true;
        zombie.wind_up = None;
    });
    give_crawler_body(commands, &assets.0, entity);
}

/// Kill every Zombie, Hellhound and Boss in play now, each an Unpaid Kill: it counts toward the
/// Round and the Run Record and pays no Points, and drops nothing. The Horde still to spawn is not
/// touched, so it still comes. Not the Nuke: no Power-up, no bonus, no announcer. The Host judges it.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct KillAllZombies;

/// The damage a `KillAllZombies` deals: finite, so a `Heavyweight`, which shrugs off an infinite
/// one-blow kill (see `heavyweight_damage`), dies of it too.
const KILL_ALL_DAMAGE: f32 = 1.0e30;

/// Each `KillAllZombies` hits every Zombie still alive for more than it could have, as an Unpaid Kill.
fn kill_all(
    mut requests: MessageReader<KillAllZombies>,
    zombies: Query<Entity, (With<Zombie>, Without<Dying>)>,
    mut damage: MessageWriter<DamageZombie>,
) {
    if requests.read().count() == 0 {
        return;
    }
    for zombie in &zombies {
        damage.write(DamageZombie {
            player: None,
            zombie,
            damage: KILL_ALL_DAMAGE,
            zone: HitZone::Torso,
            attack: Attack::Nuke,
        });
    }
}

/// A hound in its lightning strike can't be hurt. Insta-Kill only doubles a Napalm's or a
/// `Heavyweight`'s damage, and a `Heavyweight` shrugs off one-blow kills (see `heavyweight_damage`). A Nova
/// Crawler's death is also told to the Nova module, which decides whether it bursts.
pub fn apply_damage(
    mut commands: Commands,
    mut requests: MessageReader<DamageZombie>,
    mut zombies: Query<
        (&mut Zombie, &Children, &Transform, Option<&Rising>, Has<Surge>, Option<&DamageTakenFactor>, Option<&Heavyweight>),
        (Without<Dying>, Without<Telegraph>),
    >,
    hit_areas: Query<(), With<HitArea>>,
    power_ups: Res<ActivePowerUps>,
    mut hits: MessageWriter<ZombieHit>,
    mut kills: MessageWriter<ZombieKilled>,
    mut nova_kills: MessageWriter<NovaCrawlerKilled>,
    mut kill_damage: MessageWriter<KillDamage>,
) {
    let insta_kill = power_ups.active(PowerUp::InstaKill);
    for request in requests.read() {
        let Ok((mut zombie, children, transform, rising, surge, factor, heavyweight)) = zombies.get_mut(request.zombie) else {
            continue;
        };
        // Several hits can land in one frame; only the first lethal one counts.
        if zombie.health <= 0.0 {
            continue;
        }
        let damage = taken_damage(request.damage, request.zone, factor);
        let overflow = overflow_damage(zombie.health, damage);
        zombie.health -= if let Some(shares) = heavyweight {
            heavyweight_damage(damage, request.attack, zombie.max_health, insta_kill, shares)
        } else if zombie.special == Some(SpecialKind::Napalm) {
            doubled_damage(damage, insta_kill)
        } else {
            dealt_damage(damage, insta_kill)
        };
        if zombie.health > 0.0 {
            // A Burn's ticks pay no hit Points, nor does a torn Plank.
            if !matches!(request.attack, Attack::Burn | Attack::Barrier) {
                zombie.hits = zombie.hits.saturating_add(1);
                hits.write(ZombieHit {
                    player: request.player,
                    pays: hit_pays(zombie.special, zombie.hits),
                    headshot: is_headshot(request.zone, request.attack),
                });
            }
            continue;
        }

        // One killed mid-drop from a Vent (or mid-rise) dies where it would have landed, so a
        // Power-up it drops is on the floor.
        let mut centre = transform.translation;
        if let Some(rising) = rising {
            centre.y = rising.standing;
        }
        let at = centre - Vec3::Y * zombie.half_height();
        kills.write(ZombieKilled {
            player: request.player,
            pay: request.attack.pay(),
            headshot: is_headshot(request.zone, request.attack),
            zone: request.zone,
            attack: request.attack,
            at,
            surge,
            zombie: request.zombie,
        });
        kill_damage.write(KillDamage {
            player: request.player,
            zombie: request.zombie,
            at,
            max_health: zombie.max_health,
            overflow,
            attack: request.attack,
            damage: request.damage,
            headshot: is_headshot(request.zone, request.attack),
        });
        if zombie.nova {
            nova_kills.write(NovaCrawlerKilled {
                player: request.player,
                zombie: request.zombie,
                at: transform.translation,
                attack: request.attack,
            });
        }
        commands
            .entity(request.zombie)
            .insert(Dying {
                killed_by: Some(request.attack),
                killer: request.player,
                headless: request.zone == HitZone::Head,
                ..default()
            })
            .remove::<(Collider, CollisionLayers)>();
        for child in children.iter() {
            if hit_areas.contains(child) {
                commands.entity(child).remove::<(Collider, CollisionLayers)>();
            }
        }
    }
}

/// Applies the Player's `Stat::ZombieHealth` entries to each Zombie that has just entered play, of
/// any kind but a requested one: a request carries its final health (a Boss's own rules, a Splitter's
/// Walkers from its already-scaled health, a Pack Alpha's hounds at the tier's health). Neutral at 1.
fn scale_spawn_health(
    mut new: Query<&mut Zombie, (Added<Zombie>, Without<Requested>)>,
    players: Query<&Modifiers, With<LocalPlayer>>,
) {
    let Some(modifiers) = players.iter().next() else {
        return;
    };
    let factor = resolve(Stat::ZombieHealth, 1.0, &modifiers.0);
    if factor == 1.0 {
        return;
    }
    for mut zombie in &mut new {
        zombie.scale_health(factor);
    }
}

/// Tells the world of each Zombie that has just entered play.
fn announce_spawns(
    new: Query<(Entity, &Zombie, Has<Requested>), Added<Zombie>>,
    round: Option<Res<Round>>,
    mut spawned: MessageWriter<ZombieSpawned>,
) {
    let round = round.map_or(1, |round| round.number);
    for (zombie, body, requested) in &new {
        spawned.write(ZombieSpawned { zombie, kind: ZombieKind::of(body), round, requested });
    }
}

/// Each `HealZombie` restores health to a Zombie that's still alive.
fn heal_zombies(mut heals: MessageReader<HealZombie>, mut zombies: Query<&mut Zombie, Without<Dying>>) {
    for heal in heals.read() {
        if let Ok(mut zombie) = zombies.get_mut(heal.zombie)
            && zombie.health > 0.0
        {
            zombie.health = healed_health(zombie.health, zombie.max_health, heal.amount);
        }
    }
}

/// Whether a dying body rolls onto its side rather than tipping backwards: a Crawler, a Hellhound or
/// a Nova Crawler, already low. A fallen Turned ally is no longer a `Zombie`, and tips backwards.
fn rolls_over(zombie: Option<&Zombie>) -> bool {
    zombie.is_some_and(|zombie| zombie.crawler || zombie.hellhound || zombie.nova)
}

/// A killed Zombie tips backwards about its local X axis, or rolls onto its side, turning a little
/// every frame (in `FrameTurnSystems`), then disappears. `sink` lowers it onto the floor. A
/// skinned body falls by its death clip instead, so its Zombie stays upright.
fn fall_over(
    mut commands: Commands,
    mut dying: Query<(Entity, &mut Dying, &mut Transform, Option<&Zombie>, Has<skin::Skinned>)>,
    time: Res<Time>,
) {
    for (entity, mut dying, mut transform, zombie, skinned) in &mut dying {
        let before = (dying.elapsed / ZOMBIE_FALL_SECS).min(1.0);
        dying.elapsed += time.delta_secs();
        let after = (dying.elapsed / ZOMBIE_FALL_SECS).min(1.0);

        let step = (after - before) * FRAC_PI_2;
        if skin::dies_by_clip(skinned, zombie) {
            // The clip falls it over.
        } else if step > 0.0 && rolls_over(zombie) {
            transform.rotate_local_z(step);
        } else if step > 0.0 {
            transform.rotate_local_x(step);
        }

        if dying.elapsed >= ZOMBIE_FALL_SECS + ZOMBIE_CORPSE_SECS {
            commands.entity(entity).despawn();
        }
    }
}

/// A body tipping backwards sinks as it falls, so it lies on the floor. In the physics step, in
/// time with `fall_over`.
fn sink(mut dying: Query<(&mut Dying, &mut Transform, Option<&Zombie>, Has<skin::Skinned>)>, time: Res<Time>) {
    for (mut dying, mut transform, zombie, skinned) in &mut dying {
        let before = (dying.sunk / ZOMBIE_FALL_SECS).min(1.0);
        dying.sunk += time.delta_secs();
        let after = (dying.sunk / ZOMBIE_FALL_SECS).min(1.0);
        if after > before && !rolls_over(zombie) && !skin::dies_by_clip(skinned, zombie) {
            transform.translation.y -= (after - before) * (body_half_height() - ZOMBIE_RADIUS);
        }
    }
}

pub mod rig;
pub mod skin;

#[cfg(test)]
pub mod headless;

#[cfg(test)]
mod hit_grace;

#[cfg(test)]
mod tests {
    use super::*;

    const STEP: f32 = 0.1;

    /// Zombies of every kind wade through Shallow Water at ×0.7, the Sapper included, and are
    /// freed the moment they step out.
    #[test]
    fn a_zombie_in_shallow_water_is_slowed_and_freed_as_it_steps_out() {
        let mut sim = headless::Sim::on(MapId::Cove, 60.0, |_| {});
        // The stand-in Player waits on the dry sand north of the surf.
        let world = sim.world_mut();
        for mut transform in world.query_filtered::<&mut Transform, With<crate::players::player::Player>>().iter_mut(world) {
            transform.translation = Vec3::new(-8.0, 0.9, 1.0);
        }
        let wet = sim.spawn(Vec3::new(-8.0, 0.0, -11.0), None);
        let sapper = sim.spawn(Vec3::new(-4.0, 0.0, -11.0), None);
        sim.world_mut().entity_mut(sapper).insert(Heavyweight::SAPPER);
        let dry = sim.spawn(Vec3::new(4.0, 0.0, -3.0), None);
        sim.run(3);
        let factor = |sim: &mut headless::Sim, zombie| sim.world_mut().get::<ZombieMoveFactor>(zombie).map(ZombieMoveFactor::factor);
        assert_eq!(factor(&mut sim, wet), Some(WADE_SPEED_MULT));
        assert_eq!(factor(&mut sim, sapper), Some(WADE_SPEED_MULT), "the Sapper wades too");
        assert_eq!(factor(&mut sim, dry), None);
        // Walking out of the surf toward the Player, it's freed.
        let mut slowed_for = 0;
        for _ in 0..sim.frames(20.0) {
            sim.run(1);
            let feet_z = sim.world_mut().get::<Transform>(wet).unwrap().translation.z - 0.9;
            match factor(&mut sim, wet) {
                Some(f) => {
                    assert_eq!(f, WADE_SPEED_MULT);
                    assert!(feet_z < -7.0, "still slowed on the sand at {feet_z}");
                    slowed_for += 1;
                }
                None if feet_z > -6.9 => break,
                None => {}
            }
        }
        assert!(slowed_for > 30, "was only slowed for {slowed_for} frames");
        let feet_z = sim.world_mut().get::<Transform>(wet).unwrap().translation.z - 0.9;
        assert!(feet_z > -7.0, "never walked out of the surf: {feet_z}");
        assert_eq!(factor(&mut sim, wet), None, "the slow stayed on once out");
    }

    #[test]
    fn a_zombie_without_a_move_factor_moves_at_its_speed() {
        assert_eq!(moved_speed(3.0, None), 3.0);
        assert_eq!(moved_speed(3.0, Some(&ZombieMoveFactor::default())), 3.0);
    }

    #[test]
    fn move_factors_from_different_sources_multiply_and_one_source_holds_one() {
        let mut factor = ZombieMoveFactor::default();
        factor.set(Source("slow"), 0.5);
        factor.set(Source("fast"), 1.25);
        assert!((moved_speed(4.0, Some(&factor)) - 2.5).abs() < 1e-6);
        // Setting a source again replaces its factor rather than stacking it.
        factor.set(Source("slow"), 0.5);
        assert!((moved_speed(4.0, Some(&factor)) - 2.5).abs() < 1e-6);
        factor.clear(Source("slow"));
        assert!((moved_speed(4.0, Some(&factor)) - 5.0).abs() < 1e-6);
        assert!(!factor.is_empty());
        factor.clear(Source("fast"));
        assert!(factor.is_empty());
    }

    /// A Zombie is past `first` only on the edge's own floor: walking level out of the top node of
    /// a drop it is, but on another floor directly under or over the edge it never is.
    #[test]
    fn a_zombie_on_another_floor_is_never_past_a_node() {
        let (top, bottom) = (Vec3::new(0.0, 4.0, 0.0), Vec3::new(3.0, 0.0, 0.0));
        assert!(already_past(top, bottom, Vec3::new(1.0, 4.0, 0.0)), "walking out to the lip");
        assert!(already_past(top, bottom, Vec3::new(1.0, 3.0, 0.0)), "mid-fall");
        let level = Vec3::new(3.0, 4.0, 0.0);
        assert!(already_past(top, level, Vec3::new(1.0, 4.0, 0.0)), "walking level");
        assert!(!already_past(top, level, Vec3::new(1.0, 0.0, 0.0)), "on the floor under the edge");
        assert!(!already_past(top, level, Vec3::new(1.0, 8.0, 0.0)), "on the floor over the edge");
    }

    /// Runs the stuck watch for `secs`, with the Zombie at `at(t)` and the Player at `player`.
    /// Returns when it fired, if it did.
    fn watch(secs: f32, player: Vec3, at: impl Fn(f32) -> Vec3) -> Option<f32> {
        let mut watch = StuckWatch::new(at(0.0));
        let mut t = 0.0;
        while t < secs {
            t += STEP;
            if watch.update(at(t), player, STEP) {
                return Some(t);
            }
        }
        None
    }

    #[test]
    fn a_zombie_that_makes_no_progress_for_ten_seconds_is_cleaned_up() {
        let fired = watch(20.0, Vec3::new(10.0, 0.0, 0.0), |_| Vec3::ZERO).unwrap();
        assert!((fired - ZOMBIE_CLEANUP_SECS).abs() < STEP * 1.5, "fired at {fired}");
    }

    #[test]
    fn a_zombie_that_keeps_walking_is_never_stuck() {
        // Walking at a Walker's pace back and forth across a room.
        let pace = |t: f32| Vec3::new((t * ZOMBIE_WALK_SPEED / 8.0).sin() * 8.0, 0.0, 0.0);
        assert_eq!(watch(60.0, Vec3::new(0.0, 0.0, 20.0), pace), None);
    }

    #[test]
    fn a_zombie_at_the_player_is_never_stuck() {
        assert_eq!(watch(60.0, Vec3::new(1.0, 0.0, 0.0), |_| Vec3::ZERO), None);
    }

    #[test]
    fn a_zombie_lost_far_from_the_player_is_cleaned_up_even_while_moving() {
        let wander = |t: f32| Vec3::new(50.0 + t, 0.0, 0.0);
        let fired = watch(20.0, Vec3::ZERO, wander).unwrap();
        assert!((fired - ZOMBIE_CLEANUP_SECS).abs() < STEP * 1.5, "fired at {fired}");
    }

    #[test]
    fn progress_restarts_the_stuck_clock() {
        // Stuck for 8 s, then a step forward, then stuck again: it fires 10 s after the step.
        let at = |t: f32| if t < 8.0 { Vec3::ZERO } else { Vec3::new(2.0, 0.0, 0.0) };
        let fired = watch(30.0, Vec3::new(10.0, 0.0, 0.0), at).unwrap();
        assert!((fired - 18.0).abs() < STEP * 1.5, "fired at {fired}");
    }

    /// Runs `update_on_route` for `secs`, with the Zombie at `at(t)` and `remaining(t)` of route
    /// left, at `speed`. Returns when it fired, if it did.
    fn on_route(secs: f32, speed: f32, at: impl Fn(f32) -> Vec3, remaining: impl Fn(f32) -> f32) -> Option<f32> {
        let mut watch = StuckWatch::new(at(0.0));
        let mut t = 0.0;
        while t < secs {
            t += STEP;
            if watch.update_on_route(at(t), Vec3::new(500.0, 0.0, 0.0), remaining(t), speed, STEP) {
                return Some(t);
            }
        }
        None
    }

    #[test]
    fn a_crawler_left_far_behind_is_not_lost_while_it_keeps_crawling() {
        // The Player runs off for 10 s at 9 m/s, then stops; the Crawler's route is 100 m at its end.
        let remaining = |t: f32| if t < 10.0 { 9.0 * t } else { 90.0 - (t - 10.0) * CRAWLER_SPEED };
        let at = |t: f32| Vec3::new(t * CRAWLER_SPEED, 0.0, 0.0);
        assert_eq!(on_route(80.0, CRAWLER_SPEED, at, remaining), None);
    }

    #[test]
    fn a_zombie_far_along_its_route_and_not_closing_in_is_lost_even_while_moving() {
        let at = |t: f32| Vec3::new(t * CRAWLER_SPEED, 0.0, 0.0);
        let fired = on_route(200.0, CRAWLER_SPEED, at, |_| 80.0).unwrap();
        assert!((fired - lost_secs(CRAWLER_SPEED)).abs() < STEP * 1.5, "fired at {fired}");
        let quick = on_route(200.0, 6.0, at, |_| 80.0).unwrap();
        assert!((quick - ZOMBIE_LOST_SECS).abs() < STEP * 1.5, "fired at {quick}");
        assert!(lost_secs(CRAWLER_SPEED) > lost_secs(ZOMBIE_WALK_SPEED));
    }

    #[test]
    fn a_zombie_wedged_on_its_route_is_cleaned_up_far_or_near() {
        for remaining in [10.0, 80.0] {
            let fired = on_route(30.0, CRAWLER_SPEED, |_| Vec3::ZERO, |_| remaining).unwrap();
            assert!((fired - ZOMBIE_CLEANUP_SECS).abs() < STEP * 1.5, "fired at {fired}");
        }
    }

    #[test]
    fn route_remaining_adds_up_the_walk() {
        let waypoints = [Vec3::new(3.0, 0.0, 0.0), Vec3::new(3.0, 0.0, 4.0)];
        assert_eq!(route_remaining(Vec3::ZERO, &waypoints, Vec3::new(3.0, 0.0, 10.0)), 3.0 + 4.0 + 6.0);
        assert_eq!(route_remaining(Vec3::ZERO, &[], Vec3::new(0.0, 0.0, 5.0)), 5.0);
    }

    #[test]
    fn a_survived_explosive_hit_of_a_tenth_of_its_health_has_an_even_chance_to_gib() {
        assert!(blows_legs_off(true, true, 1000.0, 100.0, 0.0));
        assert!(blows_legs_off(true, true, 1000.0, 999.0, 0.49));
        assert!(!blows_legs_off(true, true, 1000.0, 100.0, 0.5), "a failed roll keeps its legs");
        assert!(!blows_legs_off(true, true, 1000.0, 99.0, 0.0), "under a tenth of its health");
    }

    #[test]
    fn a_killing_blast_never_gibs() {
        assert!(!blows_legs_off(true, true, 300.0, 300.0, 0.0));
        assert!(!blows_legs_off(true, true, 300.0, 5000.0, 0.0));
    }

    #[test]
    fn only_normal_zombies_gib() {
        assert!(!blows_legs_off(true, false, 1000.0, 500.0, 0.0));
    }

    #[test]
    fn with_crawlers_off_explosives_never_gib() {
        let gibs = (0..1000).filter(|i| blows_legs_off(false, true, 1000.0, 500.0, *i as f64 / 1000.0)).count();
        assert_eq!(gibs, 0);
    }

    #[test]
    fn about_half_of_qualifying_hits_gib() {
        let gibs = (0..1000).filter(|i| blows_legs_off(true, true, 1000.0, 500.0, *i as f64 / 1000.0)).count();
        assert_eq!(gibs, 500);
    }

    #[test]
    fn a_crawler_is_slow_and_never_sprints() {
        let mut zombie = Zombie::new(150.0, SpeedTier::Walker, Vec3::ZERO);
        zombie.crawler = true;
        assert_eq!(zombie.speed(), CRAWLER_SPEED);
        zombie.sprint();
        assert_eq!(zombie.speed(), CRAWLER_SPEED, "a lone last-Zombie Crawler never sprints");
        let mut normal = Zombie::new(150.0, SpeedTier::Walker, Vec3::ZERO);
        normal.sprint();
        assert_eq!(normal.speed(), ZOMBIE_SPRINT_SPEED);
    }

    #[test]
    fn the_player_can_jump_over_a_crawler() {
        let apex = PLAYER_JUMP_SPEED * PLAYER_JUMP_SPEED / (2.0 * GRAVITY);
        let height = 2.0 * CRAWLER_RADIUS;
        assert!((height - 0.6).abs() < 0.05, "about 0.6 m tall");
        assert!(apex > height, "jumps {apex} m over {height} m");
    }

    #[test]
    fn ten_seconds_at_a_planked_barrier_is_stuck() {
        let (mut waited, mut stuck) = (0.0, false);
        let mut t = 0.0;
        while !stuck && t < 20.0 {
            t += STEP;
            (waited, stuck) = planked_wait(waited, true, STEP);
        }
        assert!((t - CRAWLER_PLANKED_STUCK_SECS).abs() < STEP * 1.5, "stuck at {t}");
    }

    #[test]
    fn an_open_barrier_resets_the_planked_wait() {
        let (waited, _) = planked_wait(9.5, true, STEP);
        assert_eq!(planked_wait(waited, false, STEP), (0.0, false));
    }

    #[test]
    fn separation_pushes_only_close_zombies_apart() {
        assert_eq!(separation(Vec3::ZERO, [Vec3::new(3.0, 0.0, 0.0)]), Vec3::ZERO);
        let push = separation(Vec3::ZERO, [Vec3::new(0.5, 0.0, 0.0)]);
        assert!(push.x < 0.0 && push.y == 0.0 && push.z == 0.0);
    }

    #[test]
    fn a_kill_overflows_by_the_damage_past_the_health_left() {
        assert_eq!(overflow_damage(100.0, 150.0), 50.0);
        assert_eq!(overflow_damage(100.0, 100.0), 0.0);
        assert_eq!(overflow_damage(100.0, 40.0), 0.0);
        assert_eq!(overflow_damage(100.0, f32::INFINITY), f32::INFINITY);
    }

    #[test]
    fn trap_and_teleporter_kills_are_unpaid_and_never_headshots() {
        assert_eq!(Attack::Trap.pay(), Pay::Unpaid);
        assert_eq!(Attack::Teleporter.pay(), Pay::Unpaid);
        assert_eq!(Attack::NovaBlast.pay(), Pay::Unpaid);
        assert!(!is_headshot(HitZone::Head, Attack::NovaBlast));
        assert!(!is_headshot(HitZone::Head, Attack::Trap));
        for attack in [Attack::Bullet, Attack::Knife, Attack::Burn] {
            assert_eq!(attack.pay(), Pay::Paid);
        }
    }

    #[test]
    fn napalm_and_death_scream_kills_are_unpaid() {
        assert_eq!(Attack::Napalm.pay(), Pay::Unpaid);
        assert_eq!(Attack::Scream.pay(), Pay::Unpaid);
    }

    #[test]
    fn a_damage_taken_factor_scales_body_and_head_hits_and_is_neutral_when_absent() {
        assert_eq!(taken_damage(100.0, HitZone::Torso, None), 100.0);
        assert_eq!(DamageTakenFactor::default(), DamageTakenFactor { body: 1.0, head: 1.0 });
        let factor = DamageTakenFactor { body: 1.25, head: 2.0 };
        assert_eq!(taken_damage(100.0, HitZone::Torso, Some(&factor)), 125.0);
        assert_eq!(taken_damage(100.0, HitZone::Limb, Some(&factor)), 125.0);
        assert_eq!(taken_damage(100.0, HitZone::Neck, Some(&factor)), 125.0);
        assert_eq!(taken_damage(100.0, HitZone::Head, Some(&factor)), 200.0);
        // Before Insta-Kill: a factored hit still kills outright under it.
        assert!(dealt_damage(taken_damage(1.0, HitZone::Torso, Some(&factor)), true).is_infinite());
    }

    #[test]
    fn insta_kill_only_doubles_a_napalms_damage() {
        assert_eq!(doubled_damage(100.0, true), 200.0);
        assert_eq!(doubled_damage(100.0, false), 100.0);
    }

    #[test]
    fn specials_are_never_normal_and_a_napalm_never_sprints() {
        let mut napalm = Zombie::special(SpecialKind::Napalm, 600.0, Vec3::ZERO);
        assert_eq!(napalm.speed(), ZOMBIE_WALK_SPEED);
        napalm.sprint();
        assert_eq!(napalm.speed(), ZOMBIE_WALK_SPEED);
        assert!(napalm.is_special() && !napalm.can_lose_legs());
        let shrieker = Zombie::special(SpecialKind::Shrieker, 375.0, Vec3::ZERO);
        assert_eq!(shrieker.speed(), ZOMBIE_SPRINT_SPEED);
        assert!(shrieker.is_special() && !shrieker.can_lose_legs());
        let normal = Zombie::new(150.0, SpeedTier::Walker, Vec3::ZERO);
        assert!(!normal.is_special() && normal.can_lose_legs());
    }

    #[test]
    fn the_sapper_takes_double_damage_from_insta_kill_and_a_twentieth_from_a_killing_ammo_mod() {
        assert_eq!(sapper_damage(100.0, Attack::Bullet, 8000.0, false), 100.0);
        assert_eq!(sapper_damage(100.0, Attack::Bullet, 8000.0, true), 200.0);
        assert_eq!(sapper_damage(f32::INFINITY, Attack::AmmoMod(Pay::Paid), 8000.0, false), 400.0);
        assert_eq!(sapper_damage(f32::INFINITY, Attack::Napalm, 8000.0, false), 400.0);
        assert_eq!(sapper_damage(40.0, Attack::Napalm, 8000.0, false), 40.0, "fire ticks hurt as usual");
    }

    /// Any Heavyweight takes its own shares: an always-kill (a Blast, a Nuke-like kill) its Ammo
    /// Mod share, a Knife kill roll its Knife share, and plain damage as dealt, however big.
    #[test]
    fn a_heavyweight_takes_its_share_from_each_always_kill() {
        let boss = Heavyweight { trap_share: 0.2, ammo_mod_share: 0.05, knife_kill_share: 0.02 };
        let max = 10_000.0;
        assert_eq!(heavyweight_damage(f32::INFINITY, Attack::AmmoMod(Pay::Unpaid), max, false, &boss), 500.0);
        assert_eq!(heavyweight_damage(f32::INFINITY, Attack::Bullet, max, false, &boss), 500.0, "an always-kill Bullet");
        assert_eq!(heavyweight_damage(f32::INFINITY, Attack::Knife, max, false, &boss), 200.0, "a Knife kill roll");
        assert_eq!(heavyweight_damage(f32::INFINITY, Attack::Knife, max, true, &boss), 400.0, "Insta-Kill doubles it");
        assert_eq!(heavyweight_damage(3_000.0, Attack::Bullet, max, false, &boss), 3_000.0, "plain damage stays uncapped");
        assert_eq!(heavyweight_damage(75.0, Attack::Knife, max, false, &boss), 75.0, "a plain Knife hit");
        assert_eq!(Heavyweight::SAPPER.trap_share, TRAP_SAPPER_HEALTH_FRACTION);
        assert_eq!(Heavyweight::SAPPER.ammo_mod_share, SAPPER_AMMO_MOD_HEALTH_FRACTION);
    }

    #[test]
    fn the_sapper_slams_three_metres_out_in_a_sixty_degree_arc() {
        let forward = Vec3::NEG_Z;
        assert!(slam_reaches(Vec3::ZERO, forward, Vec3::new(0.0, 0.5, -2.9)));
        assert!(!slam_reaches(Vec3::ZERO, forward, Vec3::new(0.0, 0.0, -3.1)), "too far");
        assert!(!slam_reaches(Vec3::ZERO, forward, Vec3::new(0.0, 0.0, 2.0)), "behind it");
        // 30° either side of its facing.
        assert!(slam_reaches(Vec3::ZERO, forward, Vec3::new(1.0, 0.0, -2.0)));
        assert!(!slam_reaches(Vec3::ZERO, forward, Vec3::new(2.0, 0.0, -2.0)));
        let sapper = Zombie::special(SpecialKind::Sapper, 8000.0, Vec3::ZERO);
        assert_eq!(sapper.attack_stats(), (3.0, 0.8, 150.0, 2.5));
    }

    #[test]
    fn the_sapper_walks_at_its_own_speed_and_never_sprints_or_crawls() {
        let mut sapper = Zombie::special(SpecialKind::Sapper, 8000.0, Vec3::ZERO);
        assert_eq!(sapper.speed(), SAPPER_SPEED);
        sapper.sprint();
        assert_eq!(sapper.speed(), 4.5);
        assert!(sapper.is_sapper() && !sapper.can_lose_legs());
        assert_eq!(sapper.max_health(), 8000.0);
    }

    #[test]
    fn zombies_walk_up_to_the_player_and_stop_in_reach() {
        assert_eq!(heading(10.0, ZOMBIE_REACH, false, false), Heading::Toward);
        assert_eq!(heading(ZOMBIE_REACH * 0.5, ZOMBIE_REACH, false, false), Heading::Stay);
    }

    #[test]
    fn a_swinging_zombie_plants_its_feet() {
        assert_eq!(heading(10.0, ZOMBIE_REACH, true, false), Heading::Stay);
    }

    #[test]
    fn zombies_back_off_to_about_15_m_from_a_player_in_last_stand() {
        assert_eq!(heading(0.5, ZOMBIE_REACH, false, true), Heading::Away);
        assert_eq!(heading(14.9, ZOMBIE_REACH, true, true), Heading::Away);
        assert_eq!(heading(15.0, ZOMBIE_REACH, false, true), Heading::Stay);
        assert_eq!(heading(40.0, ZOMBIE_REACH, false, true), Heading::Stay);
    }

    #[test]
    fn a_hellhound_stops_further_out_and_bites_a_second_apart() {
        assert_eq!(heading(2.2, ZOMBIE_REACH, false, false), Heading::Toward);
        assert_eq!(heading(1.9, HELLHOUND_REACH, false, false), Heading::Stay);
        let hound = Zombie::hellhound(400.0, Vec3::ZERO);
        let (reach, wind_up, damage, cooldown) = hound.attack_stats();
        assert_eq!((reach, damage), (2.5, 40.0));
        assert!((wind_up + cooldown - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_hellhound_runs_at_its_own_speed_and_never_crawls() {
        let mut hound = Zombie::hellhound(400.0, Vec3::ZERO);
        assert_eq!(hound.speed(), HELLHOUND_SPEED);
        hound.sprint();
        assert_eq!(hound.speed(), HELLHOUND_SPEED);
        assert!(hound.is_hellhound() && !hound.is_crawler());
    }

    #[test]
    fn a_nova_crawler_is_quick_low_swipes_for_45_and_never_crawls() {
        let mut nova = Zombie::nova_crawler(100.0, Vec3::ZERO);
        assert_eq!(nova.speed(), NOVA_CRAWLER_SPEED);
        nova.sprint();
        assert_eq!(nova.speed(), NOVA_CRAWLER_SPEED, "the last Zombie sprinting doesn't change it");
        assert_eq!(nova.attack_stats().2, 45.0);
        assert!((2.0 * nova.half_height() - 0.6).abs() < 0.05, "about 0.6 m tall");
        assert!(nova.is_nova_crawler() && !nova.is_crawler());
        assert!(!nova.can_lose_legs(), "it never becomes a Crawler");
        assert!(Zombie::new(150.0, SpeedTier::Walker, Vec3::ZERO).can_lose_legs());
    }

    /// The teammate of the two-Players harness, Crouched where they stand: the real stance step
    /// lowers their body, so their centre and feet are the Crouched ones.
    fn crouched_teammate() -> (headless::Sim, Entity, Entity) {
        use crate::players::player::{Locomotion, MoveInput, Stance};
        use bevy::ecs::system::RunSystemOnce;
        let (mut sim, local, teammate) = crate::players::player::two_players::two_players();
        sim.world_mut().get_mut::<MoveInput>(teammate).unwrap().crouch = true;
        sim.run(10);
        assert_eq!(sim.world_mut().get::<Locomotion>(teammate).unwrap().stance, Stance::Crouched);
        // Half a body lower than the standing one's, with the feet still on the floor.
        let spot = sim
            .world_mut()
            .run_system_once(move |players: Players| players.get(teammate).unwrap())
            .unwrap();
        // The feet are the centre less the Crouched Stance's half-height — not the standing
        // capsule's — so they stay on the floor the body stands on.
        let crouched = PLAYER_RADIUS + CROUCH_COLLIDER_LENGTH / 2.0;
        assert!(
            (spot.feet.y - (spot.centre.y - crouched)).abs() < 1e-6,
            "the feet are the centre less the standing half-height: {:?} vs {:?}",
            spot.feet,
            spot.centre
        );
        (sim, local, teammate)
    }

    /// Cover `tall` between the teammate and a Zombie on its other side, knee-high unless told
    /// otherwise.
    fn cover_between(sim: &mut headless::Sim, tall: f32) {
        sim.world_mut().spawn((
            RigidBody::Static,
            Collider::cuboid(0.1, tall, 4.0),
            Transform::from_xyz(-3.45, tall / 2.0, 20.0),
            CollisionLayers::new(GameLayer::World, LayerMask::ALL),
        ));
    }

    /// A Zombie walking at the teammate from the cover's other side, and every swipe it lands.
    fn swipes_at(mut sim: headless::Sim, secs: f32) -> Vec<PlayerHit> {
        sim.spawn(Vec3::new(-4.8, 0.0, 20.0), None);
        let mut hits = Vec::new();
        for _ in 0..sim.frames(secs) {
            sim.run(1);
            hits.extend(sim.world_mut().resource_mut::<Messages<PlayerHit>>().drain());
        }
        hits
    }

    #[test]
    fn a_crouched_player_behind_knee_high_cover_still_gets_swiped() {
        let (mut sim, _local, teammate) = crouched_teammate();
        cover_between(&mut sim, 0.75);
        let hits = swipes_at(sim, 3.0);
        assert!(hits.iter().all(|hit| hit.player == teammate), "somebody else was swiped");
        assert!(!hits.is_empty(), "the knee-high cover shielded a Crouched Player from the swipe");
    }

    #[test]
    fn a_swipe_comes_from_the_zombie_that_swung() {
        let (mut sim, _local, teammate) = crate::players::player::two_players::two_players();
        let player = sim.world_mut().get::<Transform>(teammate).expect("a Player").translation;
        cover_between(&mut sim, 0.75);
        let hits = swipes_at(sim, 3.0);
        assert!(!hits.is_empty(), "no swipe landed");
        for hit in hits {
            let from = hit.from.expect("a swipe has a source");
            // The Zombie came in from −X, along the line to the Player, and swung from its reach.
            assert!(from.x < player.x, "{from} isn't on the Zombie's side of {player}");
            assert!((from.z - player.z).abs() < 1.0, "{from} is off the Zombie's line to {player}");
        }
    }

    #[test]
    fn a_wall_still_stops_the_swipe_whatever_the_stance() {
        let (mut sim, _local, _teammate) = crouched_teammate();
        cover_between(&mut sim, 2.5);
        let hits = swipes_at(sim, 3.0);
        assert!(hits.is_empty(), "a swipe crossed a full-height wall");
    }

    /// A Zombie far from the stand-in Player, with a Turned ally standing in its reach.
    fn zombie_beside_an_ally() -> (headless::Sim, Entity, Entity) {
        use crate::weapons::ammo_mod::TurnedAlly;
        let mut sim = headless::Sim::new(60.0, |_| {});
        let feet = crate::maps::map::PLAYER_START + Vec3::new(-6.0, 0.0, 0.0);
        let zombie = sim.spawn(feet, None);
        let ally = sim
            .world_mut()
            .spawn((TurnedAlly::default(), Transform::from_translation(feet + Vec3::new(0.8, body_half_height(), 0.0))))
            .id();
        (sim, zombie, ally)
    }

    #[test]
    fn a_zombie_swings_at_a_turned_ally_in_its_reach() {
        let (mut sim, zombie, ally) = zombie_beside_an_ally();
        let mut hits = Vec::new();
        for _ in 0..sim.frames(2.0) {
            sim.run(1);
            hits.extend(sim.world_mut().resource_mut::<Messages<TurnedAllyHit>>().drain());
        }
        assert!(!hits.is_empty(), "the Zombie never swung at the ally beside it");
        assert!(hits.iter().all(|hit| hit.ally == ally && hit.by == zombie));
        // Swinging plants its feet, and a landed swing waits out its cooldown like any other.
        let secs = ZOMBIE_WIND_UP_SECS + ZOMBIE_HIT_COOLDOWN_SECS;
        assert!(hits.len() as f32 <= 2.0 / secs + 1.0, "{} swings in 2 s", hits.len());
    }

    #[test]
    fn a_swing_at_a_turned_ally_never_hits_a_player() {
        let (mut sim, ..) = zombie_beside_an_ally();
        sim.run(sim.frames(2.0));
        assert!(sim.world_mut().resource_mut::<Messages<PlayerHit>>().drain().next().is_none());
    }

    #[test]
    fn a_standing_player_behind_the_same_cover_is_swiped_too() {
        // Not crouched: the chest-height ray is their centre's height either way.
        let (mut sim, _local, _teammate) = crate::players::player::two_players::two_players();
        cover_between(&mut sim, 0.75);
        let hits = swipes_at(sim, 3.0);
        assert!(!hits.is_empty(), "the cover shielded a standing Player");
    }

    #[test]
    fn a_heal_never_goes_past_max_health_or_below_none() {
        assert_eq!(healed_health(50.0, 100.0, 30.0), 80.0);
        assert_eq!(healed_health(90.0, 100.0, 30.0), 100.0);
        assert_eq!(healed_health(50.0, 100.0, -30.0), 50.0);
    }

    #[test]
    fn heal_zombie_restores_a_live_zombies_health_only() {
        let mut app = App::new();
        app.add_message::<HealZombie>().add_systems(Update, heal_zombies);
        let mut hurt = Zombie::new_for_test(100.0);
        hurt.set_health_for_test(40.0);
        let live = app.world_mut().spawn(hurt).id();
        let mut dying = Zombie::new_for_test(100.0);
        dying.set_health_for_test(0.0);
        let dead = app.world_mut().spawn((dying, Dying::default())).id();
        for zombie in [live, dead] {
            app.world_mut().write_message(HealZombie { zombie, amount: 25.0 });
        }
        app.update();
        assert_eq!(app.world().get::<Zombie>(live).unwrap().health(), 65.0);
        assert_eq!(app.world().get::<Zombie>(dead).unwrap().health(), 0.0);
    }

    #[test]
    fn kill_all_kills_every_zombie_in_play_unpaid_even_a_heavyweight() {
        use crate::enemies::zombie::headless::Sim;
        #[derive(Resource, Default)]
        struct Kills(Vec<(Attack, Pay)>);
        let mut sim = Sim::new(60.0, |app| {
            app.init_resource::<Kills>().add_systems(Update, |mut kills: ResMut<Kills>, mut heard: MessageReader<ZombieKilled>| {
                kills.0.extend(heard.read().map(|kill| (kill.attack, kill.pay)));
            });
        });
        let walker = sim.spawn(Vec3::new(10.0, 0.0, 10.0), None);
        let heavy = sim.spawn(Vec3::new(12.0, 0.0, 10.0), None);
        sim.world_mut().entity_mut(heavy).insert(Heavyweight::SAPPER);
        sim.run(3);
        sim.send(KillAllZombies);
        sim.run(3);
        for zombie in [walker, heavy] {
            assert!(sim.world_mut().get::<Dying>(zombie).is_some(), "{zombie:?} is dying");
        }
        assert_eq!(sim.world_mut().resource::<Kills>().0, [(Attack::Nuke, Pay::Unpaid), (Attack::Nuke, Pay::Unpaid)]);
    }

    #[test]
    fn a_new_zombie_is_announced_once_with_its_kind_and_whether_it_was_requested() {
        let mut app = App::new();
        app.add_message::<ZombieSpawned>().insert_resource(Round::in_progress(7)).add_systems(Update, announce_spawns);
        let normal = app.world_mut().spawn(Zombie::new_for_test(100.0)).id();
        let hound = app.world_mut().spawn((Zombie::hellhound(100.0, Vec3::ZERO), Requested::default())).id();
        app.update();
        app.update();
        let told: Vec<_> = app.world_mut().resource_mut::<Messages<ZombieSpawned>>().drain().map(|s| (s.zombie, s.kind, s.round, s.requested)).collect();
        assert_eq!(told.len(), 2, "each told once");
        assert!(told.contains(&(normal, ZombieKind::Normal, 7, false)));
        assert!(told.contains(&(hound, ZombieKind::Hellhound, 7, true)));
    }

    fn thick_hide(factor: f32) -> Modifiers {
        use crate::modifier::{Effect, Modifier};
        let entry = Modifier { stat: Stat::ZombieHealth, source: Source("Test"), effect: Effect::Mult(factor) };
        Modifiers(vec![entry])
    }

    #[test]
    fn zombie_health_scales_every_new_kind_but_a_requested_one() {
        let mut app = App::new();
        app.add_systems(Update, scale_spawn_health);
        app.world_mut().spawn((LocalPlayer, thick_hide(1.5)));
        let normal = app.world_mut().spawn(Zombie::new_for_test(100.0)).id();
        let hound = app.world_mut().spawn((Zombie::hellhound(100.0, Vec3::ZERO), Requested::default())).id();
        let sapper = app.world_mut().spawn(Zombie::special(SpecialKind::Sapper, 100.0, Vec3::ZERO)).id();
        let boss = app.world_mut().spawn((Zombie::new_for_test(100.0), Requested::default(), Heavyweight::SAPPER)).id();
        app.update();
        let health = |app: &App, zombie| {
            let zombie = app.world().get::<Zombie>(zombie).unwrap();
            (zombie.health(), zombie.max_health())
        };
        assert_eq!(health(&app, normal), (150.0, 150.0));
        assert_eq!(health(&app, hound), (100.0, 100.0), "a requested hound already carries its final health");
        assert_eq!(health(&app, sapper), (150.0, 150.0), "a Sapper that isn't a Boss is thick too");
        assert_eq!(health(&app, boss), (100.0, 100.0), "a Boss isn't");
        app.update();
        assert_eq!(health(&app, normal), (150.0, 150.0), "once, at spawn");
    }

    #[test]
    fn zombie_health_is_neutral_without_entries() {
        let mut app = App::new();
        app.add_systems(Update, scale_spawn_health);
        app.world_mut().spawn((LocalPlayer, Modifiers::default()));
        let normal = app.world_mut().spawn(Zombie::new_for_test(100.0)).id();
        app.update();
        assert_eq!(app.world().get::<Zombie>(normal).unwrap().max_health(), 100.0);
    }

    #[test]
    fn a_zombies_kind_is_read_from_its_body() {
        assert_eq!(ZombieKind::of(&Zombie::special(SpecialKind::Sapper, 10.0, Vec3::ZERO)), ZombieKind::Special(SpecialKind::Sapper));
        assert_eq!(ZombieKind::of(&Zombie::nova_crawler(10.0, Vec3::ZERO)), ZombieKind::NovaCrawler);
        assert_eq!(ZombieKind::of(&Zombie::new_for_test(10.0)), ZombieKind::Normal);
    }
}
