use std::f32::consts::FRAC_PI_2;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_transform_interpolation::{TransformEasingSystems, TranslationEasingState};

use crate::authority::HostSimulation;
use crate::door::Door;
use crate::layers::GameLayer;
use crate::modifier::{Stat, resolve};
use crate::barrier::{Barriers, Entering};
use crate::explosive::in_sight;
use crate::hellhound::Telegraph;
use crate::nav::{NavGraph, Route, repath_interval};
use crate::target::{Players, nearest_up_player};
use crate::nova::NovaCrawlerKilled;
use crate::player::PlayerHit;
use crate::power_up::{ActivePowerUps, PowerUp};
use crate::round::CarriesOver;
use crate::special::{SpecialCue, SpecialKind, hit_pays};
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// The damage a hit really does to the Sapper, with `max_health`. Insta-Kill only doubles it. An
/// Ammo Mod proc, which would kill anything else, deals `SAPPER_AMMO_MOD_HEALTH_FRACTION` of its
/// max health instead, and so does another Special's one-blow kill (a Napalm's blast, a death
/// scream, a Nova Crawler's burst).
pub fn sapper_damage(damage: f32, attack: Attack, max_health: f32, insta_kill: bool) -> f32 {
    let instant = matches!(attack, Attack::AmmoMod(_))
        || (matches!(attack, Attack::Napalm | Attack::Scream | Attack::NovaBlast) && !damage.is_finite());
    let damage = if instant { max_health * SAPPER_AMMO_MOD_HEALTH_FRACTION } else { damage };
    doubled_damage(damage, insta_kill)
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
}

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
}

impl StuckWatch {
    pub fn new(at: Vec3) -> Self {
        Self { anchor: at, secs: 0.0 }
    }

    /// A watch about to run out on a Zombie at `at` that makes no progress.
    #[cfg(test)]
    pub fn nearly_up(at: Vec3) -> Self {
        Self {
            anchor: at,
            secs: ZOMBIE_CLEANUP_SECS - 0.05,
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
        }
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
    fn half_height(&self) -> f32 {
        if self.hellhound {
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
/// last step left it (see `crate::player::Teleport`). Any other change to a Zombie's Transform
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
                (rise, chase, tumble, sink).in_set(HostSimulation).run_if(world_runs),
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
                    apply_damage,
                    clean_up_stuck,
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
    let buried = standing - (half + head_offset + ZOMBIE_HEAD_RADIUS + 0.05);
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

/// Whether a Zombie-sized body can walk straight from `from` to `to` without hitting the Map.
fn clear_walk(spatial: &SpatialQuery, probe: &Collider, from: Vec3, to: Vec3) -> bool {
    let delta = to - from;
    let Ok(dir) = Dir3::new(delta) else {
        return true;
    };
    spatial
        .cast_shape(
            probe,
            from,
            Quat::IDENTITY,
            dir,
            &ShapeCastConfig::from_max_distance(delta.length()),
            &SpatialQueryFilter::from_mask(GameLayer::World),
        )
        .is_none()
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

/// Whether a Zombie at `feet`, planning from its nearest node `first` to the next one `second`, is
/// already on the edge between them and past `first`. Halfway up a ramp the nearest node is still
/// the one behind, and without this the Zombie keeps turning back for it and never climbs. No cast
/// is needed: the Map's tests check every edge is walkable, and a straight cast clips a ramp.
fn already_past(first: Vec3, second: Vec3, feet: Vec3) -> bool {
    let edge = second - first;
    let along = (feet - first).dot(edge) / edge.length_squared();
    along > 0.0 && feet.distance(first + edge * along.min(1.0)) < ZOMBIE_WAYPOINT_REACH
}

/// Zombies find a Player (or the Monkey Bomb pulling them) anywhere on the Map: every so often they
/// pick the nearest Player by path who is up, and re-plan a route over the path-node graph. They
/// steer straight at the Player when there's a clear walk or they share a node area, and move
/// under gravity with grounded move-and-slide.
fn chase(
    mut zombies: Query<
        (Entity, &mut Zombie, &mut Route, &mut Transform, &Collider, Option<&Lured>, Option<&Seeking>),
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
    barriers: Res<Barriers>,
    move_and_slide: MoveAndSlide,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    let open_doors: Vec<_> = doors.iter().filter(|door| door.open).filter_map(|door| door.joins).collect();
    let probe = Collider::sphere(ZOMBIE_RADIUS * 0.8);
    let filter_mask = [GameLayer::World, GameLayer::Zombie, GameLayer::Player];
    let positions: Vec<_> = zombies.iter().map(|(entity, _, _, transform, ..)| (entity, transform.translation)).collect();

    for (entity, mut zombie, mut route, mut transform, collider, lured, seeking) in &mut zombies {
        let half = zombie.half_height();
        let feet = transform.translation - Vec3::Y * half;

        // Picks the nearest Player who is up, again every so often so a Train can pass between
        // Players, and at once when its Player goes down. With none up (a solo Last Stand) it
        // stays with the nearest, and leaves them alone.
        route.retarget_in -= dt;
        let held = route.target.and_then(|target| players.get(target));
        if route.retarget_in <= 0.0 || held.is_none_or(|player| !player.up) {
            route.retarget_in = ZOMBIE_RETARGET_SECS;
            let picked = nearest_up_player(&graph, feet, players.iter(), &open_doors, |b| barriers.is_open(b))
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
        let target_feet = goal_feet.unwrap_or(player.feet);
        let last_stand = !player.up && goal_feet.is_none();
        let rise = target_feet.y - feet.y;

        route.repath_in -= dt;
        if route.repath_in <= 0.0 {
            route.repath_in = repath_interval(feet.distance(target_feet));
            let from = graph.nearest(feet);
            let to = graph.nearest(target_feet);
            let level = rise.abs() <= ZOMBIE_DIRECT_MAX_RISE;
            let target = target_feet + Vec3::Y * half;
            let spatial = &move_and_slide.spatial_query;
            route.waypoints.clear();
            if !(level && (from == to || clear_walk(spatial, &probe, transform.translation, target))) {
                // No route at all (the drop is one-way) leaves it steering straight at its target
                // until the stuck rule replaces it.
                let path = graph.plan(from, to, &open_doors, |b| barriers.is_open(b)).unwrap_or_default();
                route.waypoints = path.into_iter().map(|i| graph.nodes[i].at).collect();
                // Skip the first node when the second is a clear walk on the same floor, or the
                // Zombie is already on the edge between them (a ramp changes floor on the way), so
                // it never doubles back to the node behind it.
                if let [first, second, ..] = route.waypoints[..]
                    && (already_past(first, second, feet)
                        || ((second.y - feet.y).abs() < 0.5
                            && (first.y - feet.y).abs() < 0.5
                            && clear_walk(spatial, &probe, transform.translation, second + Vec3::Y * half)))
                {
                    route.waypoints.remove(0);
                }
            }
        }

        // Reached the next node: on to the one after.
        if let Some(&next) = route.waypoints.first()
            && horizontal(next - feet).length() < ZOMBIE_WAYPOINT_REACH
            && (next.y - feet.y).abs() < 1.5
        {
            route.waypoints.remove(0);
        }
        let goal = route.waypoints.first().copied().unwrap_or(target_feet);

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
        let speed = resolve(Stat::ZombieSpeed, zombie.speed(), modifiers);
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

/// A Zombie that's stuck, or lost far from the Player, is quietly removed and sent back into the
/// Horde, so a Round never stalls. A Zombie outside a window is waiting on its Planks, not
/// stuck, so its clock only starts once it's in. A Crawler can't tear them, though, so for it
/// waiting at a planked Barrier too long is stuck. A Zombie that carries over between Rounds is
/// never cleaned up here: its own module moves it instead.
fn clean_up_stuck(
    mut commands: Commands,
    mut zombies: Query<
        (Entity, &mut Zombie, &Transform, Option<&Entering>),
        (Without<Dying>, Without<Rising>, Without<Telegraph>, Without<CarriesOver>),
    >,
    players: Players,
    barriers: Res<Barriers>,
    mut cleaned: MessageWriter<ZombieCleanedUp>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    for (entity, mut zombie, transform, entering) in &mut zombies {
        if let Some(entering) = entering {
            zombie.stuck = StuckWatch::new(transform.translation);
            let planked = zombie.crawler && entering.at_barrier() && barriers.planks(entering.barrier) > 0;
            let stuck;
            (zombie.planked_wait, stuck) = planked_wait(zombie.planked_wait, planked, dt);
            if stuck {
                commands.entity(entity).despawn();
                cleaned.write(ZombieCleanedUp);
            }
            continue;
        }
        let Some(player) = players.nearest(transform.translation) else {
            continue;
        };
        if zombie.stuck.update(transform.translation, player.centre, dt) {
            commands.entity(entity).despawn();
            cleaned.write(ZombieCleanedUp);
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

/// Zombies inside swipe the Player in reach, Hellhounds bite, and the Sapper slams the Player in
/// the arc in front of it, all only with a clear line to the Player: a wall or a closed Door
/// stops a swipe and its wind-up. Those outside a window reach through it instead (see the
/// Barriers).
fn attack(
    mut zombies: Query<
        (&mut Zombie, &Transform, Option<&Route>),
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
    spatial: SpatialQuery,
    mut hits: MessageWriter<PlayerHit>,
    mut cues: MessageWriter<SpecialCue>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    for (mut zombie, transform, route) in &mut zombies {
        let Some(player) = players.target_of(route.and_then(|route| route.target), transform.translation) else {
            continue;
        };
        // Zombies leave a Player in Last Stand alone.
        if !player.up {
            zombie.wind_up = None;
            continue;
        }
        let (reach, wind_up, damage, cooldown) = zombie.attack_stats();
        // Same floor only: a Zombie under the Balcony can't swipe the Player standing on it. The
        // line runs chest to chest (both transforms are capsule centres), so low cover like a
        // Yard crate doesn't block it, but a wall does.
        let in_reach = horizontal(player.centre - transform.translation).length() <= reach
            && (player.centre.y - transform.translation.y).abs() < 1.5
            && (!zombie.is_sapper() || slam_reaches(transform.translation, *transform.forward(), player.centre))
            && in_sight(&spatial, transform.translation, player.centre);
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
                // A Zombie that lands a swipe is ignored for a moment; other Zombies can still hit.
                if in_reach {
                    hits.write(PlayerHit {
                        player: player.entity,
                        damage,
                    });
                    // The Sapper's slam has its own sound, above.
                    if !zombie.is_sapper() {
                        let cue = if zombie.hellhound {
                            Cue::HoundBite
                        } else if zombie.nova {
                            Cue::NovaAttack
                        } else {
                            Cue::ZombieAttack
                        };
                        cues.write(SpecialCue { cue, at });
                    }
                    zombie.attack_cooldown = cooldown;
                }
            }
            Some(left) => zombie.wind_up = Some(left - dt),
            None if in_reach && zombie.attack_cooldown <= 0.0 => {
                zombie.wind_up = Some(wind_up);
                if zombie.is_sapper() {
                    cues.write(SpecialCue { cue: Cue::SapperWindup, at });
                }
            }
            None => {}
        }
    }
}

/// A knock-down drops any swing in progress. A Riser still climbing out, or a hound still in its
/// lightning, can't be knocked down. The Sapper keeps its feet: the Thundergun stuns it instead.
fn knock_down(
    mut commands: Commands,
    mut requests: MessageReader<KnockDown>,
    mut zombies: Query<
        (&mut Zombie, &Transform),
        (Without<Dying>, Without<Rising>, Without<KnockedDown>, Without<Telegraph>),
    >,
    mut stuns: MessageWriter<StunZombie>,
) {
    for request in requests.read() {
        let Ok((mut zombie, transform)) = zombies.get_mut(request.zombie) else {
            continue;
        };
        if zombie.is_sapper() {
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
        let mut body = None;
        commands
            .entity(entity)
            .insert((Mesh3d(assets.crawler_body.clone()), Collider::sphere(CRAWLER_RADIUS)))
            .despawn_children()
            .with_children(|crawler| body = Some(spawn_crawler_parts(crawler, &assets, entity)));
        if let Some(body) = body {
            commands.entity(entity).insert(body);
        }
    }
}

/// A hound in its lightning strike can't be hurt. Insta-Kill only doubles a Napalm's or the
/// Sapper's damage, and the Sapper shrugs off one-blow kills (see `sapper_damage`). A Nova
/// Crawler's death is also told to the Nova module, which decides whether it bursts.
pub fn apply_damage(
    mut commands: Commands,
    mut requests: MessageReader<DamageZombie>,
    mut zombies: Query<(&mut Zombie, &Children, &Transform, Option<&Rising>), (Without<Dying>, Without<Telegraph>)>,
    hit_areas: Query<(), With<HitArea>>,
    power_ups: Res<ActivePowerUps>,
    mut hits: MessageWriter<ZombieHit>,
    mut kills: MessageWriter<ZombieKilled>,
    mut nova_kills: MessageWriter<NovaCrawlerKilled>,
    mut kill_damage: MessageWriter<KillDamage>,
) {
    let insta_kill = power_ups.active(PowerUp::InstaKill);
    for request in requests.read() {
        let Ok((mut zombie, children, transform, rising)) = zombies.get_mut(request.zombie) else {
            continue;
        };
        // Several hits can land in one frame; only the first lethal one counts.
        if zombie.health <= 0.0 {
            continue;
        }
        let overflow = overflow_damage(zombie.health, request.damage);
        zombie.health -= if zombie.is_sapper() {
            sapper_damage(request.damage, request.attack, zombie.max_health, insta_kill)
        } else if zombie.special == Some(SpecialKind::Napalm) {
            doubled_damage(request.damage, insta_kill)
        } else {
            dealt_damage(request.damage, insta_kill)
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
        });
        kill_damage.write(KillDamage {
            player: request.player,
            zombie: request.zombie,
            at,
            max_health: zombie.max_health,
            overflow,
            attack: request.attack,
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

/// Whether a dying body rolls onto its side rather than tipping backwards: a Crawler, a Hellhound or
/// a Nova Crawler, already low. A fallen Turned ally is no longer a `Zombie`, and tips backwards.
fn rolls_over(zombie: Option<&Zombie>) -> bool {
    zombie.is_some_and(|zombie| zombie.crawler || zombie.hellhound || zombie.nova)
}

/// A killed Zombie tips backwards about its local X axis, or rolls onto its side, turning a little
/// every frame (in `FrameTurnSystems`), then disappears. `sink` lowers it onto the floor.
fn fall_over(
    mut commands: Commands,
    mut dying: Query<(Entity, &mut Dying, &mut Transform, Option<&Zombie>)>,
    time: Res<Time>,
) {
    for (entity, mut dying, mut transform, zombie) in &mut dying {
        let before = (dying.elapsed / ZOMBIE_FALL_SECS).min(1.0);
        dying.elapsed += time.delta_secs();
        let after = (dying.elapsed / ZOMBIE_FALL_SECS).min(1.0);

        let step = (after - before) * FRAC_PI_2;
        if step > 0.0 && rolls_over(zombie) {
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
fn sink(mut dying: Query<(&mut Dying, &mut Transform, Option<&Zombie>)>, time: Res<Time>) {
    for (mut dying, mut transform, zombie) in &mut dying {
        let before = (dying.sunk / ZOMBIE_FALL_SECS).min(1.0);
        dying.sunk += time.delta_secs();
        let after = (dying.sunk / ZOMBIE_FALL_SECS).min(1.0);
        if after > before && !rolls_over(zombie) {
            transform.translation.y -= (after - before) * (body_half_height() - ZOMBIE_RADIUS);
        }
    }
}

mod rig;

#[cfg(test)]
pub mod headless;

#[cfg(test)]
mod tests {
    use super::*;

    const STEP: f32 = 0.1;

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
}
