use std::f32::consts::FRAC_PI_2;

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::door::Door;
use crate::last_stand::LastStand;
use crate::layers::GameLayer;
use crate::barrier::{Barriers, Entering};
use crate::nav::{NavGraph, Route, repath_interval};
use crate::player::{Player, PlayerHit};
use crate::state::{InRun, Pause};
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
    /// An Ammo Mod's own damage, Paid or Unpaid by the Ammo Mod. It never rolls Ammo Mods.
    AmmoMod(Pay),
}

impl Attack {
    /// Whether a kill by this attack pays kill Points.
    pub fn pay(self) -> Pay {
        match self {
            Attack::AmmoMod(pay) => pay,
            Attack::Bullet | Attack::Knife | Attack::Burn => Pay::Paid,
            Attack::Trap => Pay::Unpaid,
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

/// Request to damage a Zombie, written by the Player's attacks.
#[derive(Message)]
pub struct DamageZombie {
    pub zombie: Entity,
    pub damage: f32,
    pub zone: HitZone,
    pub attack: Attack,
}

/// Request to knock a Zombie off its feet, as the Thundergun's blast does: it's flung along
/// `push` and lies there a moment before it gets up.
#[derive(Message)]
pub struct KnockDown {
    pub zombie: Entity,
    pub push: Vec3,
}

/// A Zombie took damage and survived.
#[derive(Message, Debug, Clone, Copy)]
pub struct ZombieHit;

/// An explosion blew a Zombie's legs off: it becomes a Crawler.
#[derive(Message, Debug, Clone, Copy)]
pub struct Gibbed {
    pub zombie: Entity,
}

/// The Crawler rule: whether a blast from an explosive that can make Crawlers blows a Zombie's
/// legs off. Only a normal Zombie (not already a Crawler) that survives a hit of at least
/// `CRAWLER_GIB_MIN_FRACTION` of its pre-hit `health` can lose them, on a `roll` in `[0, 1)`
/// under `CRAWLER_GIB_CHANCE`.
pub fn blows_legs_off(normal: bool, health: f32, damage: f32, roll: f64) -> bool {
    normal && damage < health && damage >= health * CRAWLER_GIB_MIN_FRACTION && roll < CRAWLER_GIB_CHANCE
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
    pub pay: Pay,
    pub headshot: bool,
    pub zone: HitZone,
    pub attack: Attack,
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
}

impl Zombie {
    fn new(health: f32, tier: SpeedTier, at: Vec3) -> Self {
        Self {
            health,
            tier,
            attack_cooldown: 0.0,
            wind_up: None,
            fall_speed: 0.0,
            stuck: StuckWatch::new(at),
            crawler: false,
            planked_wait: 0.0,
        }
    }

    /// How fast it moves: by its Speed Tier, or `CRAWLER_SPEED` once it's a Crawler.
    pub fn speed(&self) -> f32 {
        if self.crawler { CRAWLER_SPEED } else { self.tier.speed() }
    }

    /// Health left; at or below 0 once killed.
    pub fn health(&self) -> f32 {
        self.health
    }

    /// Switches to Sprinter, as the last Zombie of an early Round does. A Crawler never sprints.
    pub fn sprint(&mut self) {
        if !self.crawler {
            self.tier = SpeedTier::Sprinter;
        }
    }

    pub fn is_crawler(&self) -> bool {
        self.crawler
    }

    /// From its centre down to its feet.
    fn half_height(&self) -> f32 {
        if self.crawler { CRAWLER_RADIUS } else { body_half_height() }
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

/// A Riser climbing out of the ground. It can be shot, but can't move or attack until it's up.
#[derive(Component)]
pub struct Rising {
    elapsed: f32,
    /// Its height when it spawned, and once fully up.
    buried: f32,
    standing: f32,
}

/// A killed Zombie falling over before it disappears.
#[derive(Component, Default)]
pub struct Dying {
    elapsed: f32,
}

#[derive(Resource)]
struct ZombieAssets {
    body: Handle<Mesh>,
    head: Handle<Mesh>,
    arm: Handle<Mesh>,
    eye: Handle<Mesh>,
    crawler_body: Handle<Mesh>,
    skin: Handle<StandardMaterial>,
    clothes: Handle<StandardMaterial>,
    eyes: Handle<StandardMaterial>,
}

pub struct ZombiePlugin;

impl Plugin for ZombiePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<DamageZombie>()
            .add_message::<ZombieHit>()
            .add_message::<ZombieKilled>()
            .add_message::<ZombieCleanedUp>()
            .add_message::<KnockDown>()
            .add_message::<Gibbed>()
            .add_systems(Startup, load_assets)
            .add_systems(FixedUpdate, (chase, tumble).run_if(in_state(Pause::Running)))
            .add_systems(
                Update,
                (
                    rise,
                    attack,
                    knock_down,
                    // First, so a Zombie killed in the same frame dies as the Crawler it became.
                    become_crawler.before(apply_damage),
                    apply_damage,
                    fall_over,
                    clean_up_stuck,
                )
                    .run_if(in_state(Pause::Running)),
            );
    }
}

fn load_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(ZombieAssets {
        body: meshes.add(Capsule3d::new(ZOMBIE_RADIUS, ZOMBIE_CAPSULE_LENGTH)),
        head: meshes.add(Sphere::new(ZOMBIE_HEAD_RADIUS)),
        arm: meshes.add(Cuboid::from_size(ZOMBIE_ARM_SIZE)),
        eye: meshes.add(Sphere::new(0.035)),
        // Lying along its facing (-Z), dragging itself forward.
        crawler_body: meshes.add(
            Mesh::from(Capsule3d::new(CRAWLER_RADIUS * 0.9, CRAWLER_BODY_LENGTH))
                .rotated_by(Quat::from_rotation_x(FRAC_PI_2)),
        ),
        skin: materials.add(Color::srgb(0.45, 0.52, 0.38)),
        clothes: materials.add(Color::srgb(0.22, 0.24, 0.2)),
        eyes: materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.6, 0.1),
            emissive: LinearRgba::rgb(8.0, 4.0, 0.5),
            ..default()
        }),
    });
}

fn body_half_height() -> f32 {
    ZOMBIE_RADIUS + ZOMBIE_CAPSULE_LENGTH / 2.0
}

/// Spawns a Zombie standing on the floor at `at`, or, for a Riser, buried under it and about to
/// climb out.
pub fn spawn_zombie(
    commands: &mut Commands,
    assets: &ZombieAssetsParam,
    at: Vec3,
    health: f32,
    tier: SpeedTier,
    rises: bool,
) -> Entity {
    let assets = &assets.0;
    let half = body_half_height();
    let head_offset = half + ZOMBIE_HEAD_RADIUS * 0.5;
    // Start just above the floor; gravity settles it.
    let standing = at.y + half + 0.02;
    // A Riser starts with the top of its head below the floor.
    let buried = standing - (half + head_offset + ZOMBIE_HEAD_RADIUS + 0.05);
    let centre = Vec3::new(at.x, if rises { buried } else { standing }, at.z);

    let mut zombie = commands.spawn_empty();
    if rises {
        zombie.insert(Rising {
            elapsed: 0.0,
            buried,
            standing,
        });
    }
    zombie
        .insert((
            InRun,
            Name::new("Zombie"),
            Zombie::new(health, tier, centre),
            Route::default(),
            Mesh3d(assets.body.clone()),
            MeshMaterial3d(assets.clothes.clone()),
            Transform::from_translation(centre),
            RigidBody::Kinematic,
            CustomPositionIntegration,
            TranslationInterpolation,
            Collider::capsule(ZOMBIE_RADIUS, ZOMBIE_CAPSULE_LENGTH),
            CollisionLayers::new(GameLayer::Zombie, LayerMask::ALL),
        ))
        .with_children(|zombie| {
            let hit_layers = CollisionLayers::new(GameLayer::ZombieHitArea, LayerMask::ALL);
            zombie
                .spawn((
                    HitArea(HitZone::Head),
                    Mesh3d(assets.head.clone()),
                    MeshMaterial3d(assets.skin.clone()),
                    Transform::from_xyz(0.0, head_offset, 0.0),
                    Collider::sphere(ZOMBIE_HEAD_RADIUS),
                    hit_layers,
                ))
                .with_children(|head| {
                    for x in [-0.07, 0.07] {
                        head.spawn((
                            Mesh3d(assets.eye.clone()),
                            MeshMaterial3d(assets.eyes.clone()),
                            Transform::from_xyz(x, 0.04, -ZOMBIE_HEAD_RADIUS * 0.9),
                        ));
                    }
                });
            // Arms reaching forward (-Z is forward).
            for x in [-ZOMBIE_RADIUS - 0.05, ZOMBIE_RADIUS + 0.05] {
                zombie.spawn((
                    HitArea(HitZone::Limb),
                    Mesh3d(assets.arm.clone()),
                    MeshMaterial3d(assets.skin.clone()),
                    Transform::from_xyz(x, half * 0.55, -0.3),
                    box_collider(ZOMBIE_ARM_SIZE),
                    hit_layers,
                ));
            }

            // Invisible hit areas inside the body capsule, stacked from the feet up:
            // legs, then torso, then a short neck under the head.
            let neck_bottom = head_offset - ZOMBIE_HEAD_RADIUS - ZOMBIE_NECK_SIZE.y;
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
            zombie.spawn((
                HitArea(HitZone::Neck),
                Transform::from_xyz(0.0, neck_bottom + ZOMBIE_NECK_SIZE.y / 2.0, 0.0),
                box_collider(ZOMBIE_NECK_SIZE),
                hit_layers,
            ));
        })
        .id()
}

/// A Crawler's parts, around a body lying low along its facing: head and neck at the front, arms
/// dragging it along, a torso, and no legs.
fn spawn_crawler_parts(crawler: &mut ChildSpawnerCommands, assets: &ZombieAssets) {
    let hit_layers = CollisionLayers::new(GameLayer::ZombieHitArea, LayerMask::ALL);
    let front = -(CRAWLER_BODY_LENGTH / 2.0 + CRAWLER_RADIUS);
    crawler
        .spawn((
            HitArea(HitZone::Head),
            Mesh3d(assets.head.clone()),
            MeshMaterial3d(assets.skin.clone()),
            Transform::from_xyz(0.0, CRAWLER_RADIUS - ZOMBIE_HEAD_RADIUS, front),
            Collider::sphere(ZOMBIE_HEAD_RADIUS),
            hit_layers,
        ))
        .with_children(|head| {
            for x in [-0.07, 0.07] {
                head.spawn((
                    Mesh3d(assets.eye.clone()),
                    MeshMaterial3d(assets.eyes.clone()),
                    Transform::from_xyz(x, 0.04, -ZOMBIE_HEAD_RADIUS * 0.9),
                ));
            }
        });
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
    // Arms out in front on the floor.
    for x in [-CRAWLER_RADIUS - 0.05, CRAWLER_RADIUS + 0.05] {
        crawler.spawn((
            HitArea(HitZone::Limb),
            Mesh3d(assets.arm.clone()),
            MeshMaterial3d(assets.skin.clone()),
            Transform::from_xyz(x, ZOMBIE_ARM_SIZE.y / 2.0 - CRAWLER_RADIUS, front),
            box_collider(ZOMBIE_ARM_SIZE),
            hit_layers,
        ));
    }
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

/// Where a Zombie heads, `distance` (horizontal) from the Player. It plants its feet while
/// swinging, so the Player can back out of reach. While the Player is in Last Stand, Zombies walk
/// away until they're about `LAST_STAND_ZOMBIE_RETREAT_DISTANCE` off.
pub fn heading(distance: f32, swinging: bool, last_stand: bool) -> Heading {
    if last_stand {
        if distance < LAST_STAND_ZOMBIE_RETREAT_DISTANCE { Heading::Away } else { Heading::Stay }
    } else if swinging || distance <= ZOMBIE_REACH * 0.8 {
        Heading::Stay
    } else {
        Heading::Toward
    }
}

/// Zombies find the Player (or the Monkey Bomb pulling them) anywhere on the Map: they re-plan a route over the path-node graph
/// every so often, steer straight at the Player when there's a clear walk or they share a node
/// area, and move under gravity with grounded move-and-slide.
fn chase(
    mut zombies: Query<
        (Entity, &mut Zombie, &mut Route, &mut Transform, &Collider, Option<&Lured>),
        (Without<Dying>, Without<Rising>, Without<KnockedDown>, Without<Entering>),
    >,
    player: Single<(&Transform, Has<LastStand>), (With<Player>, Without<Zombie>)>,
    graph: Res<NavGraph>,
    doors: Query<&Door>,
    barriers: Res<Barriers>,
    move_and_slide: MoveAndSlide,
    time: Res<Time>,
) {
    let (player, last_stand) = *player;
    let dt = time.delta_secs();
    let player_feet = player.translation - Vec3::Y * (PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0);
    let open_doors: Vec<_> = doors.iter().filter(|door| door.open).filter_map(|door| door.joins).collect();
    let probe = Collider::sphere(ZOMBIE_RADIUS * 0.8);
    let filter_mask = [GameLayer::World, GameLayer::Zombie, GameLayer::Player];
    let positions: Vec<_> = zombies.iter().map(|(entity, _, _, transform, _, _)| (entity, transform.translation)).collect();

    for (entity, mut zombie, mut route, mut transform, collider, lured) in &mut zombies {
        let half = zombie.half_height();
        let feet = transform.translation - Vec3::Y * half;
        // A Monkey Bomb's pull beats everything, even a Player in Last Stand.
        let target_feet = lured.map_or(player_feet, |lure| lure.0);
        let last_stand = last_stand && lured.is_none();
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
                // Skip the first node when the second is a clear walk on the same floor, so a
                // Zombie never doubles back to the node behind it.
                if let [first, second, ..] = route.waypoints[..]
                    && (second.y - feet.y).abs() < 0.5
                    && (first.y - feet.y).abs() < 0.5
                    && clear_walk(spatial, &probe, transform.translation, second + Vec3::Y * half)
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
        let head = heading(reach_distance, zombie.wind_up.is_some(), last_stand);
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
        let stride = if planted { Vec3::ZERO } else { heading * zombie.speed() };
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

/// Risers climb out of the ground over `ZOMBIE_RISE_SECS`.
fn rise(mut commands: Commands, mut risers: Query<(Entity, &mut Rising, &mut Transform), Without<Dying>>, time: Res<Time>) {
    for (entity, mut rising, mut transform) in &mut risers {
        rising.elapsed += time.delta_secs();
        let t = (rising.elapsed / ZOMBIE_RISE_SECS).min(1.0);
        transform.translation.y = rising.buried.lerp(rising.standing, t);
        if t >= 1.0 {
            commands.entity(entity).remove::<Rising>();
        }
    }
}

/// A Zombie that's stuck, or lost far from the Player, is quietly removed and sent back into the
/// Horde, so a Round never stalls. A Zombie outside a window is waiting on its Planks, not
/// stuck, so its clock only starts once it's in. A Crawler can't tear them, though, so for it
/// waiting at a planked Barrier too long is stuck.
fn clean_up_stuck(
    mut commands: Commands,
    mut zombies: Query<(Entity, &mut Zombie, &Transform, Option<&Entering>), (Without<Dying>, Without<Rising>)>,
    player: Single<&Transform, (With<Player>, Without<Zombie>)>,
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
        if zombie.stuck.update(transform.translation, player.translation, dt) {
            commands.entity(entity).despawn();
            cleaned.write(ZombieCleanedUp);
        }
    }
}

/// Zombies inside swipe the Player in reach. Those outside a window reach through it instead (see
/// the Barriers).
fn attack(
    mut zombies: Query<
        (&mut Zombie, &Transform),
        (Without<Dying>, Without<Rising>, Without<KnockedDown>, Without<Entering>),
    >,
    player: Single<(&Transform, Has<LastStand>), With<Player>>,
    mut hits: MessageWriter<PlayerHit>,
    time: Res<Time>,
) {
    let (player, last_stand) = *player;
    let dt = time.delta_secs();
    for (mut zombie, transform) in &mut zombies {
        // Zombies leave a Player in Last Stand alone.
        if last_stand {
            zombie.wind_up = None;
            continue;
        }
        // Same floor only: a Zombie under the Balcony can't swipe the Player standing on it.
        let in_reach = horizontal(player.translation - transform.translation).length() <= ZOMBIE_REACH
            && (player.translation.y - transform.translation.y).abs() < 1.5;
        zombie.attack_cooldown = (zombie.attack_cooldown - dt).max(0.0);

        match zombie.wind_up {
            Some(left) if left - dt <= 0.0 => {
                zombie.wind_up = None;
                // A Zombie that lands a swipe is ignored for a moment; other Zombies can still hit.
                if in_reach {
                    hits.write(PlayerHit {
                        damage: ZOMBIE_SWIPE_DAMAGE,
                    });
                    zombie.attack_cooldown = ZOMBIE_HIT_COOLDOWN_SECS;
                }
            }
            Some(left) => zombie.wind_up = Some(left - dt),
            None if in_reach && zombie.attack_cooldown <= 0.0 => {
                zombie.wind_up = Some(ZOMBIE_WIND_UP_SECS);
            }
            None => {}
        }
    }
}

/// A knock-down drops any swing in progress. A Riser still climbing out can't be knocked down.
fn knock_down(
    mut commands: Commands,
    mut requests: MessageReader<KnockDown>,
    mut zombies: Query<(&mut Zombie, &Transform), (Without<Dying>, Without<Rising>, Without<KnockedDown>)>,
) {
    for request in requests.read() {
        let Ok((mut zombie, transform)) = zombies.get_mut(request.zombie) else {
            continue;
        };
        zombie.wind_up = None;
        commands.entity(request.zombie).insert(KnockedDown {
            elapsed: 0.0,
            push: request.push,
            facing: transform.rotation,
        });
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
    mut zombies: Query<(&mut Zombie, &mut Transform), (Without<Dying>, Without<Rising>)>,
    assets: Res<ZombieAssets>,
) {
    for &Gibbed { zombie: entity } in gibbed.read() {
        let Ok((mut zombie, mut transform)) = zombies.get_mut(entity) else {
            continue;
        };
        if zombie.crawler {
            continue;
        }
        zombie.crawler = true;
        zombie.wind_up = None;
        transform.translation.y -= body_half_height() - CRAWLER_RADIUS;
        commands
            .entity(entity)
            .insert((Mesh3d(assets.crawler_body.clone()), Collider::sphere(CRAWLER_RADIUS)))
            .despawn_children()
            .with_children(|crawler| spawn_crawler_parts(crawler, &assets));
    }
}

pub fn apply_damage(
    mut commands: Commands,
    mut requests: MessageReader<DamageZombie>,
    mut zombies: Query<(&mut Zombie, &Children), Without<Dying>>,
    hit_areas: Query<(), With<HitArea>>,
    mut hits: MessageWriter<ZombieHit>,
    mut kills: MessageWriter<ZombieKilled>,
) {
    for request in requests.read() {
        let Ok((mut zombie, children)) = zombies.get_mut(request.zombie) else {
            continue;
        };
        // Several hits can land in one frame; only the first lethal one counts.
        if zombie.health <= 0.0 {
            continue;
        }
        zombie.health -= request.damage;
        if zombie.health > 0.0 {
            // A Burn's ticks pay no hit Points.
            if request.attack != Attack::Burn {
                hits.write(ZombieHit);
            }
            continue;
        }

        kills.write(ZombieKilled {
            pay: request.attack.pay(),
            headshot: is_headshot(request.zone, request.attack),
            zone: request.zone,
            attack: request.attack,
        });
        commands
            .entity(request.zombie)
            .insert(Dying::default())
            .remove::<(Collider, CollisionLayers)>();
        for child in children.iter() {
            if hit_areas.contains(child) {
                commands.entity(child).remove::<(Collider, CollisionLayers)>();
            }
        }
    }
}

fn fall_over(
    mut commands: Commands,
    mut dying: Query<(Entity, &mut Dying, &mut Transform, &Zombie)>,
    time: Res<Time>,
) {
    for (entity, mut dying, mut transform, zombie) in &mut dying {
        let before = (dying.elapsed / ZOMBIE_FALL_SECS).min(1.0);
        dying.elapsed += time.delta_secs();
        let after = (dying.elapsed / ZOMBIE_FALL_SECS).min(1.0);

        // Tip backwards about the local X axis, sinking so it lies on the floor. A Crawler,
        // already down, rolls onto its side.
        let step = (after - before) * FRAC_PI_2;
        if step > 0.0 && zombie.crawler {
            transform.rotate_local_z(step);
        } else if step > 0.0 {
            transform.rotate_local_x(step);
            transform.translation.y -= (after - before) * (body_half_height() - ZOMBIE_RADIUS);
        }

        if dying.elapsed >= ZOMBIE_FALL_SECS + ZOMBIE_CORPSE_SECS {
            commands.entity(entity).despawn();
        }
    }
}

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
        assert!(blows_legs_off(true, 1000.0, 100.0, 0.0));
        assert!(blows_legs_off(true, 1000.0, 999.0, 0.49));
        assert!(!blows_legs_off(true, 1000.0, 100.0, 0.5), "a failed roll keeps its legs");
        assert!(!blows_legs_off(true, 1000.0, 99.0, 0.0), "under a tenth of its health");
    }

    #[test]
    fn a_killing_blast_never_gibs() {
        assert!(!blows_legs_off(true, 300.0, 300.0, 0.0));
        assert!(!blows_legs_off(true, 300.0, 5000.0, 0.0));
    }

    #[test]
    fn only_normal_zombies_gib() {
        assert!(!blows_legs_off(false, 1000.0, 500.0, 0.0));
    }

    #[test]
    fn about_half_of_qualifying_hits_gib() {
        let gibs = (0..1000).filter(|i| blows_legs_off(true, 1000.0, 500.0, *i as f64 / 1000.0)).count();
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
    fn trap_kills_are_unpaid_and_never_headshots() {
        assert_eq!(Attack::Trap.pay(), Pay::Unpaid);
        assert!(!is_headshot(HitZone::Head, Attack::Trap));
        for attack in [Attack::Bullet, Attack::Knife, Attack::Burn] {
            assert_eq!(attack.pay(), Pay::Paid);
        }
    }

    #[test]
    fn zombies_walk_up_to_the_player_and_stop_in_reach() {
        assert_eq!(heading(10.0, false, false), Heading::Toward);
        assert_eq!(heading(ZOMBIE_REACH * 0.5, false, false), Heading::Stay);
    }

    #[test]
    fn a_swinging_zombie_plants_its_feet() {
        assert_eq!(heading(10.0, true, false), Heading::Stay);
    }

    #[test]
    fn zombies_back_off_to_about_15_m_from_a_player_in_last_stand() {
        assert_eq!(heading(0.5, false, true), Heading::Away);
        assert_eq!(heading(14.9, true, true), Heading::Away);
        assert_eq!(heading(15.0, false, true), Heading::Stay);
        assert_eq!(heading(40.0, false, true), Heading::Stay);
    }
}
