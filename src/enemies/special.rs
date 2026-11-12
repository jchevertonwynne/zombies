//! The special slot: the Napalm and the Shrieker, two Special Zombies that share one slot, at most
//! one a Round. Each keeps its own counter of the Round it's next due in; the Napalm is checked
//! first, and neither comes on a Hellhound Round. A special spawns partway through the Horde at a
//! Special Spawn Point, counts in the Horde and the Alive Cap, and carries over if it's still
//! alive when the rest of the Round is dead.
//!
//! - The **Napalm** walks up, winds up and explodes, and explodes on every death however it dies:
//!   the blast hurts the Player, Shellshocks them and kills the Zombies around it, and leaves a
//!   fire pool that kills the Zombies that walk in. It leaves a smaller pool where it rises.
//! - The **Shrieker** sprints, screams to blur and shake the Player's view, and its death scream
//!   kills the Zombies in sight around it.
//!
//! Their own kills are Unpaid. The Sapper has its own counter and lives in its own module.

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_transform_interpolation::TranslationEasingState;

use crate::weapons::arsenal::Burn;
use crate::audio::cues::Cue;
use crate::authority::HostSimulation;
use crate::weapons::burn::Burning;
use crate::weapons::explosive::in_sight;
use crate::ui::hud::overlay::{Overlay, Overlays};
use crate::maps::map::{MapId, SpawnKind, SpawnPoint};
use crate::enemies::nav::Route;
use crate::players::player::{LocalPlayer, Player, PlayerCamera, PlayerHit};
use crate::enemies::target::Players;
use crate::economy::points::BonusPoints;
use crate::run::round::{CarriesOver, Round, RoundPhase, zombie_health};
use crate::storage::settings::{Settings, SpecialType, SpecialZombies};
use crate::players::shellshock::Shellshock;
use crate::state::{GameState, InRun, world_runs};
use crate::tuning::*;
use crate::enemies::zombie::{
    Attack, DamageZombie, Dying, HitZone, KnockedDown, Pay, Rising, SpeedTier, StuckWatch, Zombie, ZombieAssetsParam,
    spawn_zombie,
};
use crate::maps::zone::{ActiveZones, ZoneId, ZoneSet};

/// The Special Zombies with a kind of their own: the two of the special slot, and the Sapper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialKind {
    Napalm,
    Shrieker,
    /// The mini-boss, on its own counter (see the Sapper module).
    Sapper,
}

impl SpecialKind {
    /// Its health in a Round whose Zombies have `health`.
    pub fn health(self, health: f32) -> f32 {
        health
            * match self {
                SpecialKind::Napalm => NAPALM_HEALTH_MULTIPLIER,
                SpecialKind::Shrieker => SHRIEKER_HEALTH_MULTIPLIER,
                SpecialKind::Sapper => SAPPER_HEALTH_MULTIPLIER,
            }
    }

    /// How long it takes to rise out of the ground. The Sapper never rises: it comes through a
    /// window.
    pub fn rise_secs(self) -> f32 {
        match self {
            SpecialKind::Napalm => NAPALM_RISE_SECS,
            SpecialKind::Shrieker => SHRIEKER_RISE_SECS,
            SpecialKind::Sapper => 0.0,
        }
    }

    /// Its On/Off switch in Settings.
    pub fn switch(self) -> SpecialType {
        match self {
            SpecialKind::Napalm => SpecialType::Napalm,
            SpecialKind::Shrieker => SpecialType::Shrieker,
            SpecialKind::Sapper => SpecialType::Sapper,
        }
    }

    /// Whether it's one of the special slot's two, rather than the Sapper.
    pub fn in_slot(self) -> bool {
        self != SpecialKind::Sapper
    }
}

/// The special slot's cadence: the Round each special is next due in. The first is drawn from
/// its first-Round range; after each death the next is a drawn gap after the Round it died in.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpecialSlot {
    napalm: u32,
    shrieker: u32,
}

impl SpecialSlot {
    /// A Run's slot, with each special's first Round.
    pub fn new(napalm_first: u32, shrieker_first: u32) -> Self {
        Self {
            napalm: napalm_first,
            shrieker: shrieker_first,
        }
    }

    /// Which special Round `round` brings, if any: at most one, the Napalm checked first, none on
    /// a Hellhound Round, and none while a special (carried over) is still alive. A type switched
    /// off in Settings (`on`) is skipped, so the other can still come; with both off the slot
    /// never fills.
    pub fn due(&self, round: u32, hellhound_round: bool, one_alive: bool, on: &SpecialZombies) -> Option<SpecialKind> {
        if hellhound_round || one_alive {
            None
        } else if on.napalm && round >= self.napalm {
            Some(SpecialKind::Napalm)
        } else if on.shrieker && round >= self.shrieker {
            Some(SpecialKind::Shrieker)
        } else {
            None
        }
    }

    /// A special of `kind` died in Round `round`: its next is due `gap` Rounds later. The Sapper
    /// keeps its own counter, so its death changes nothing here.
    pub fn died(&mut self, kind: SpecialKind, round: u32, gap: u32) {
        let next = round.saturating_add(gap);
        match kind {
            SpecialKind::Napalm => self.napalm = next,
            SpecialKind::Shrieker => self.shrieker = next,
            SpecialKind::Sapper => {}
        }
    }

    /// The gap to draw after a special of `kind` dies.
    fn draw_gap(kind: SpecialKind) -> u32 {
        match kind {
            SpecialKind::Napalm => rand::random_range(NAPALM_ROUND_GAPS),
            SpecialKind::Shrieker => rand::random_range(SHRIEKER_ROUND_GAPS),
            SpecialKind::Sapper => 0,
        }
    }
}

/// How many of a Horde of `horde` must have spawned before the special does, at `fraction` of
/// it. Never the whole Horde, so a Zombie is always left for the special to take the place of.
pub fn special_spawn_at(horde: u32, fraction: f64) -> u32 {
    ((horde as f64 * fraction).floor() as u32).min(horde.saturating_sub(1))
}

/// The Special Spawn Points a special picks from, uniformly: those in Active Zones at least
/// `SPECIAL_SPAWN_MIN_DIST` from every Player in `players` (their Zone and where they stand), a
/// Player's own Zone's first.
pub fn special_spawn_candidates(
    points: impl IntoIterator<Item = (ZoneId, Vec3)>,
    players: &[(ZoneId, Vec3)],
    active: ZoneSet,
) -> Vec<Vec3> {
    let open: Vec<(ZoneId, Vec3)> = points
        .into_iter()
        .filter(|(zone, at)| {
            active.contains(*zone) && players.iter().all(|(_, player)| at.distance(*player) >= SPECIAL_SPAWN_MIN_DIST)
        })
        .collect();
    let own: Vec<Vec3> = open
        .iter()
        .filter(|(zone, _)| players.iter().any(|(player_zone, _)| player_zone == zone))
        .map(|(_, at)| *at)
        .collect();
    if own.is_empty() { open.into_iter().map(|(_, at)| at).collect() } else { own }
}

/// Whether the `n`th hit a Zombie survives pays hit Points: never a Napalm's or the Sapper's,
/// every `SHRIEKER_HIT_POINTS_EVERY`th of a Shrieker's, and every one of anything else's.
pub fn hit_pays(special: Option<SpecialKind>, n: u32) -> bool {
    match special {
        None => true,
        Some(SpecialKind::Napalm | SpecialKind::Sapper) => false,
        Some(SpecialKind::Shrieker) => n.is_multiple_of(SHRIEKER_HIT_POINTS_EVERY),
    }
}

/// The bonus a Napalm's death pays on top of its kill Points: `NAPALM_KILL_BONUS` for the
/// Player's kill, nothing for an Unpaid one (its own self-detonation, say).
pub fn napalm_kill_bonus(pay: Pay) -> u64 {
    match pay {
        Pay::Paid => NAPALM_KILL_BONUS,
        Pay::Unpaid => 0,
    }
}

/// Whether a Napalm `age` seconds old starts its self-detonation, with the Player `offset` from it.
pub fn napalm_triggers(offset: Vec3, age: f32) -> bool {
    age >= NAPALM_DETONATE_MIN_AGE_SECS
        && offset.with_y(0.0).length() <= NAPALM_DETONATE_HORIZONTAL
        && offset.y.abs() <= NAPALM_DETONATE_VERTICAL
}

/// What a Napalm's explosion does to the Player `distance` away, if it reaches them: 250 close,
/// 100 out to 3.8 m, then falling from 100 to 50 at 10.2 m.
pub fn napalm_blast_damage(distance: f32) -> Option<f32> {
    if distance < NAPALM_BLAST_CLOSE_DIST {
        Some(NAPALM_BLAST_CLOSE_DAMAGE)
    } else if distance <= NAPALM_BLAST_MID_DIST {
        Some(NAPALM_BLAST_MID_DAMAGE)
    } else if distance <= NAPALM_BLAST_FAR_DIST {
        let t = (distance - NAPALM_BLAST_MID_DIST) / (NAPALM_BLAST_FAR_DIST - NAPALM_BLAST_MID_DIST);
        Some(NAPALM_BLAST_MID_DAMAGE + (NAPALM_BLAST_FAR_DAMAGE - NAPALM_BLAST_MID_DAMAGE) * t)
    } else {
        None
    }
}

/// Seconds of Shellshock from a Napalm's explosion `distance` away: the longest at the centre,
/// the shortest at the blast's edge.
pub fn napalm_shellshock_secs(distance: f32) -> f32 {
    let t = (distance / NAPALM_BLAST_FAR_DIST).clamp(0.0, 1.0);
    NAPALM_SHELLSHOCK_SECS.end() + (NAPALM_SHELLSHOCK_SECS.start() - NAPALM_SHELLSHOCK_SECS.end()) * t
}

/// How strongly the heat tint shows `distance` from the nearest Napalm: full at it, none from
/// `NAPALM_HEAT_DIST`.
pub fn heat_strength(distance: f32) -> f32 {
    (1.0 - distance / NAPALM_HEAT_DIST).clamp(0.0, 1.0)
}

/// Whether a point stands in a fire pool on the floor at `pool`.
pub fn in_fire(pool: Vec3, at: Vec3) -> bool {
    (at - pool).with_y(0.0).length() <= NAPALM_POOL_RADIUS && (-0.5..2.0).contains(&(at.y - pool.y))
}

/// Whether a Shrieker at `from`, facing `forward`, can scream at the Player at `to`: close enough
/// both ways and in front of it.
pub fn scream_reaches(from: Vec3, forward: Vec3, to: Vec3) -> bool {
    let offset = to - from;
    let flat = offset.with_y(0.0);
    flat.length() <= SHRIEKER_SCREAM_HORIZONTAL
        && offset.y.abs() <= SHRIEKER_SCREAM_VERTICAL
        && forward.with_y(0.0).normalize_or_zero().dot(flat.normalize_or_zero()) >= SHRIEKER_SCREAM_MIN_DOT
}

/// A sound from the special slot, for the audio to play: from `at`, or flat without it.
#[derive(Message, Debug, Clone, Copy)]
pub struct SpecialCue {
    pub cue: Cue,
    pub at: Option<Vec3>,
}

/// The special the current Round brings, and once how much of the Horde it spawns.
#[derive(Resource, Default)]
struct SpecialPlan {
    /// The Round this plan is for.
    round: u32,
    /// The special still to spawn this Round; `None` once it has, or if none is due.
    kind: Option<SpecialKind>,
    at: u32,
}

/// A Napalm Zombie: its age, and its self-detonation wind-up once started.
#[derive(Component)]
pub struct Napalm {
    age: f32,
    wind_up: Option<f32>,
}

/// A Shrieker: its scream's cooldown and wind-up, and when it next gasps.
#[derive(Component)]
pub struct Shrieker {
    cooldown: f32,
    wind_up: Option<f32>,
    idle_in: f32,
}

impl Shrieker {
    /// Whether it's winding up its scream.
    pub fn screaming(&self) -> bool {
        self.wind_up.is_some()
    }
}

/// Watches a special for being stuck. A special is never cleaned up; it's moved instead.
#[derive(Component)]
struct SpecialWatch(StuckWatch);

/// A Shrieker's death scream, going off once its delay is up.
#[derive(Component)]
struct DeathScream {
    /// The Player whose kill set it off, who its kills credit.
    killer: Option<Entity>,
    left: f32,
}

/// A fire pool on the floor: it burns the Player every tick while they stand in it.
#[derive(Component)]
pub struct FirePool {
    ticks: Burning,
    /// The Player whose kill of the Napalm left it, who the Zombies it burns credit.
    killer: Option<Entity>,
    /// The short pool a Napalm leaves where it spawns, rather than the one it leaves at death.
    pub spawned: bool,
}

/// A Zombie that walked into a fire pool, burning to death.
#[derive(Component)]
struct PoolBurning {
    killer: Option<Entity>,
    left: f32,
}

/// A Napalm's fireball, swelling and gone in a moment.
#[derive(Component)]
struct Fireball {
    age: f32,
}

const FIREBALL_SECS: f32 = 0.5;

/// Seconds the Player's view has left to shake from a scream.
#[derive(Resource, Default)]
struct ScreamShake {
    left: f32,
}

#[derive(Resource)]
struct SpecialAssets {
    napalm: Handle<StandardMaterial>,
    shrieker: Handle<StandardMaterial>,
    pool: Handle<Mesh>,
    fire: Handle<StandardMaterial>,
    ball: Handle<Mesh>,
    flame: Handle<Mesh>,
}

/// Lets a client dress its puppets as a Napalm or a Shrieker without knowing about the asset handles.
#[derive(bevy::ecs::system::SystemParam)]
pub struct SpecialAssetsParam<'w>(Res<'w, SpecialAssets>);

pub struct SpecialPlugin;

impl Plugin for SpecialPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SpecialCue>()
            .init_resource::<SpecialPlan>()
            .init_resource::<ScreamShake>()
            .insert_resource(SpecialSlot::new(*NAPALM_FIRST_ROUNDS.start(), *SHRIEKER_FIRST_ROUNDS.start()))
            .add_systems(Startup, load_assets)
            .add_systems(OnEnter(GameState::Playing), reset)
            .add_systems(FixedUpdate, unstick.in_set(HostSimulation).run_if(world_runs))
            .add_systems(
                Update,
                (
                    (plan_special, spawn_special).chain().in_set(HostSimulation),
                    (detonate, shriek, special_deaths, death_screams, fire_pools, pool_burns).in_set(HostSimulation),
                    heat_tint,
                    shake_view,
                    fade_fireballs,
                )
                    .run_if(world_runs),
            );
    }
}

pub(crate) fn load_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(SpecialAssets {
        napalm: materials.add(StandardMaterial {
            base_color: Color::srgb(0.35, 0.12, 0.04),
            emissive: LinearRgba::rgb(4.0, 1.2, 0.1),
            ..default()
        }),
        shrieker: materials.add(Color::srgb(0.78, 0.8, 0.85)),
        pool: meshes.add(Cylinder::new(NAPALM_POOL_RADIUS, 0.04)),
        fire: materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.45, 0.1, 0.6),
            emissive: LinearRgba::rgb(12.0, 4.0, 0.5),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        }),
        ball: meshes.add(Sphere::new(1.0)),
        flame: meshes.add(Sphere::new(0.25)),
    });
}

fn reset(mut commands: Commands) {
    commands.insert_resource(SpecialSlot::new(
        rand::random_range(NAPALM_FIRST_ROUNDS),
        rand::random_range(SHRIEKER_FIRST_ROUNDS),
    ));
    commands.insert_resource(SpecialPlan::default());
    commands.insert_resource(ScreamShake::default());
}

/// Each new Round decides whether it brings a special, and how far into the Horde.
fn plan_special(
    round: Option<Res<Round>>,
    slot: Res<SpecialSlot>,
    mut plan: ResMut<SpecialPlan>,
    alive: Query<&Zombie, Without<Dying>>,
    settings: Res<Settings>,
) {
    let Some(round) = round else {
        return;
    };
    if plan.round == round.number {
        return;
    }
    let one_alive = alive.iter().any(|zombie| zombie.special_kind().is_some_and(SpecialKind::in_slot));
    *plan = SpecialPlan {
        round: round.number,
        kind: slot.due(round.number, round.hellhound.is_some(), one_alive, &settings.special_zombies),
        at: special_spawn_at(round.horde(), rand::random_range(SPECIAL_SPAWN_HORDE_FRACTION)),
    };
}

/// The Round's special rises once enough of the Horde has spawned, in a Zombie's place. One
/// switched off since the Round was planned doesn't come, and its place stays a normal Zombie.
fn spawn_special(
    mut commands: Commands,
    mut plan: ResMut<SpecialPlan>,
    mut round: ResMut<Round>,
    alive: Query<(), (With<Zombie>, Without<Dying>)>,
    spawn_points: Query<(&Transform, &SpawnPoint)>,
    players: Players,
    active: Res<ActiveZones>,
    zombie_assets: ZombieAssetsParam,
    assets: Res<SpecialAssets>,
    settings: Res<Settings>,
    mut cues: MessageWriter<SpecialCue>,
) {
    let Some(kind) = plan.kind else {
        return;
    };
    // It spawns relative to every Player who is up: there's no Zombie yet to say which it is after.
    let anchors = players.spawn_anchors();
    if anchors.is_empty() {
        return;
    }
    if !settings.special_zombies.is_on(kind.switch()) {
        plan.kind = None;
        return;
    }
    if round.phase != RoundPhase::Active
        || round.horde_spawned() < plan.at
        || alive.iter().count() as u32 >= round.alive_cap()
    {
        return;
    }
    let points = special_spawn_candidates(
        spawn_points
            .iter()
            .filter(|(_, point)| point.kind == SpawnKind::Special)
            .map(|(t, point)| (point.zone, t.translation)),
        &anchors.iter().map(|spot| (spot.zone, spot.centre)).collect::<Vec<_>>(),
        active.0,
    );
    if points.is_empty() || !round.take_from_horde() {
        return;
    }
    let at = points[rand::random_range(0..points.len())];
    let health = kind.health(zombie_health(round.number) as f32);
    let tier = match kind {
        SpecialKind::Napalm | SpecialKind::Sapper => SpeedTier::Walker,
        SpecialKind::Shrieker => SpeedTier::Sprinter,
    };
    let zombie = spawn_zombie(&mut commands, &zombie_assets, at, health, tier, Some(kind.rise_secs()));
    commands.entity(zombie).insert((
        Zombie::special(kind, health, at),
        CarriesOver,
        SpecialWatch(StuckWatch::new(at)),
    ));
    match kind {
        SpecialKind::Napalm => {
            commands
                .entity(zombie)
                .insert((
                    Name::new("Napalm"),
                    Napalm {
                        age: 0.0,
                        wind_up: None,
                    },
                    MeshMaterial3d(assets.napalm.clone()),
                ))
                .with_child(napalm_glow());
            spawn_pool(&mut commands, &assets, at, None, NAPALM_SPAWN_POOL_DAMAGE, NAPALM_SPAWN_POOL_SECS);
            cues.write(SpecialCue {
                cue: Cue::NapalmRise,
                at: Some(at),
            });
        }
        SpecialKind::Shrieker => {
            commands.entity(zombie).insert((
                Name::new("Shrieker"),
                Shrieker {
                    cooldown: 0.0,
                    wind_up: None,
                    idle_in: rand::random_range(SHRIEKER_IDLE_SECS),
                },
                MeshMaterial3d(assets.shrieker.clone()),
            ));
            cues.write(SpecialCue {
                cue: Cue::ShriekerRise,
                at: Some(at),
            });
        }
        // The slot never plans the Sapper.
        SpecialKind::Sapper => {}
    }
    plan.kind = None;
}

/// The light a living Napalm gives off.
fn napalm_glow() -> (PointLight, Transform) {
    (
        PointLight {
            color: Color::srgb(1.0, 0.5, 0.15),
            intensity: 60_000.0,
            range: 6.0,
            ..default()
        },
        Transform::from_xyz(0.0, 0.3, 0.0),
    )
}

/// Dresses a puppet Zombie as a Napalm or a Shrieker, as the Host's is. Neither has any AI here:
/// all a client needs of them is how they look, and for a Napalm the heat it gives off.
pub fn dress_puppet(commands: &mut Commands, assets: &SpecialAssetsParam, entity: Entity, kind: SpecialKind) {
    let assets = &assets.0;
    let mut zombie = commands.entity(entity);
    zombie.entry::<Zombie>().and_modify(move |mut zombie| zombie.become_special(kind));
    match kind {
        SpecialKind::Napalm => {
            zombie
                .insert((Name::new("Napalm"), Napalm { age: 0.0, wind_up: None }, MeshMaterial3d(assets.napalm.clone())))
                .with_child(napalm_glow());
        }
        SpecialKind::Shrieker => {
            zombie.insert((
                Name::new("Shrieker"),
                Shrieker { cooldown: 0.0, wind_up: None, idle_in: 0.0 },
                MeshMaterial3d(assets.shrieker.clone()),
            ));
        }
        SpecialKind::Sapper => {}
    }
}

/// A fire pool on the floor at `at`, burning the Player for `damage` a tick over `secs`.
fn spawn_pool(commands: &mut Commands, assets: &SpecialAssets, at: Vec3, killer: Option<Entity>, damage: f32, secs: f32) {
    commands.spawn((
        InRun,
        Name::new("Fire pool"),
        FirePool {
            killer,
            ticks: Burning::new(Burn {
                damage,
                tick_secs: NAPALM_POOL_TICK_SECS,
                secs,
            }),
            spawned: secs < NAPALM_DEATH_POOL_SECS,
        },
        Mesh3d(assets.pool.clone()),
        MeshMaterial3d(assets.fire.clone()),
        Transform::from_translation(at + Vec3::Y * 0.03),
    ));
}

/// A Napalm near the Player winds up and blows itself up, an Unpaid death that pays nothing.
fn detonate(
    mut napalms: Query<(Entity, &mut Napalm, &Transform, Option<&Route>, Has<Rising>, Has<KnockedDown>), Without<Dying>>,
    players: Players,
    mut damage: MessageWriter<DamageZombie>,
    mut cues: MessageWriter<SpecialCue>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    for (entity, mut napalm, transform, route, rising, down) in &mut napalms {
        napalm.age += dt;
        let Some(player) = players.target_of(route.and_then(|route| route.target), transform.translation) else {
            continue;
        };
        match napalm.wind_up {
            Some(left) if left - dt <= 0.0 => {
                napalm.wind_up = None;
                // Its own blast, counted for the Player it was after.
                damage.write(DamageZombie {
                    player: Some(player.entity),
                    zombie: entity,
                    damage: f32::INFINITY,
                    zone: HitZone::Torso,
                    attack: Attack::Napalm,
                });
            }
            Some(left) => napalm.wind_up = Some(left - dt),
            None if !rising
                && !down
                && player.up
                && napalm_triggers(player.centre - transform.translation, napalm.age) =>
            {
                napalm.wind_up = Some(NAPALM_WIND_UP_SECS);
                cues.write(SpecialCue {
                    cue: Cue::NapalmWindup,
                    at: Some(transform.translation),
                });
            }
            None => {}
        }
    }
}

/// A Shrieker with the Player in front of it winds up and screams, blurring and shaking their
/// view (a Shellshock) and shaking it, then waits out its cooldown. It gasps every few seconds meanwhile.
fn shriek(
    mut shriekers: Query<(&mut Shrieker, &Transform, Option<&Route>, Has<Rising>, Has<KnockedDown>), Without<Dying>>,
    players: Players,
    spatial: SpatialQuery,
    mut shellshocks: MessageWriter<Shellshock>,
    mut shake: ResMut<ScreamShake>,
    mut cues: MessageWriter<SpecialCue>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    for (mut shrieker, transform, route, rising, down) in &mut shriekers {
        let at = transform.translation;
        let target = players.target_of(route.and_then(|route| route.target), at);
        shrieker.idle_in -= dt;
        if shrieker.idle_in <= 0.0 {
            shrieker.idle_in = rand::random_range(SHRIEKER_IDLE_SECS);
            cues.write(SpecialCue {
                cue: Cue::ShriekerIdle,
                at: Some(at),
            });
        }
        if rising || down {
            shrieker.wind_up = None;
            continue;
        }
        shrieker.cooldown = (shrieker.cooldown - dt).max(0.0);
        let reaches = target.filter(|player| {
            player.up && scream_reaches(at, *transform.forward(), player.centre) && in_sight(&spatial, at, player.centre)
        });
        match shrieker.wind_up {
            Some(left) if left - dt <= 0.0 => {
                shrieker.wind_up = None;
                shrieker.cooldown = rand::random_range(SHRIEKER_COOLDOWN_SECS);
                cues.write(SpecialCue {
                    cue: Cue::ShriekerScream,
                    at: Some(at),
                });
                // Stepping out of reach during the wind-up dodges it. The haze and shake are on
                // the screen of the Player it reached.
                if reaches.is_some_and(|player| player.local) {
                    shellshocks.write(Shellshock {
                        secs: SHRIEKER_BLUR_SECS,
                    });
                    cues.write(SpecialCue {
                        cue: Cue::ScreamHit,
                        at: None,
                    });
                    shake.left = shake.left.max(SHRIEKER_SHAKE_SECS);
                }
            }
            Some(left) => shrieker.wind_up = Some(left - dt),
            None if reaches.is_some() && shrieker.cooldown <= 0.0 => {
                shrieker.wind_up = Some(SHRIEKER_WIND_UP_SECS);
                cues.write(SpecialCue {
                    cue: Cue::ShriekerWindup,
                    at: Some(at),
                });
            }
            None => {}
        }
    }
}

/// A special's death: a Napalm explodes, whatever killed it, and a Shrieker lets out its death
/// scream. Either way its counter books its next Round.
fn special_deaths(
    mut commands: Commands,
    deaths: Query<(&Zombie, &Transform, &Dying), Added<Dying>>,
    zombies: Query<(Entity, &Transform), (With<Zombie>, Without<Dying>)>,
    players: Players,
    spatial: SpatialQuery,
    round: Option<Res<Round>>,
    mut slot: ResMut<SpecialSlot>,
    mut shellshocks: MessageWriter<Shellshock>,
    mut damage: MessageWriter<DamageZombie>,
    mut player_hits: MessageWriter<PlayerHit>,
    mut bonus: MessageWriter<BonusPoints>,
    mut cues: MessageWriter<SpecialCue>,
    assets: Res<SpecialAssets>,
) {
    let round_number = round.map_or(1, |round| round.number);
    for (zombie, transform, dying) in &deaths {
        let Some(kind) = zombie.special_kind().filter(|kind| kind.in_slot()) else {
            continue;
        };
        slot.died(kind, round_number, SpecialSlot::draw_gap(kind));
        let at = transform.translation;
        let feet = at - Vec3::Y * (ZOMBIE_RADIUS + ZOMBIE_CAPSULE_LENGTH / 2.0);
        match kind {
            SpecialKind::Napalm => {
                let pay = dying.killed_by().map_or(Pay::Unpaid, Attack::pay);
                let paid = napalm_kill_bonus(pay);
                if paid > 0
                    && let Some(killer) = dying.killer()
                {
                    bonus.write(BonusPoints {
                        player: killer,
                        amount: paid,
                    });
                }
                // The blast kills every Zombie in sight around it, Unpaid.
                for (other, other_at) in &zombies {
                    if other_at.translation.distance(at) <= NAPALM_KILL_RADIUS
                        && in_sight(&spatial, at, other_at.translation)
                    {
                        damage.write(DamageZombie {
                            player: dying.killer(),
                            zombie: other,
                            damage: f32::INFINITY,
                            zone: HitZone::Torso,
                            attack: Attack::Napalm,
                        });
                    }
                }
                for player in players.iter() {
                    let distance = player.centre.distance(at);
                    if let Some(amount) = napalm_blast_damage(distance)
                        && in_sight(&spatial, at, player.centre)
                    {
                        player_hits.write(PlayerHit {
                            player: player.entity,
                            damage: amount,
                            from: Some(at),
                        });
                        if player.local {
                            shellshocks.write(Shellshock {
                                secs: napalm_shellshock_secs(distance),
                            });
                        }
                    }
                }
                cues.write(SpecialCue {
                    cue: Cue::NapalmExplode,
                    at: Some(at),
                });
                spawn_pool(&mut commands, &assets, feet, dying.killer(), NAPALM_DEATH_POOL_DAMAGE, NAPALM_DEATH_POOL_SECS);
                commands.spawn((
                    InRun,
                    Fireball { age: 0.0 },
                    Mesh3d(assets.ball.clone()),
                    MeshMaterial3d(assets.fire.clone()),
                    Transform::from_translation(at).with_scale(Vec3::splat(0.01)),
                ));
            }
            SpecialKind::Shrieker => {
                commands.spawn((
                    InRun,
                    DeathScream {
                        killer: dying.killer(),
                        left: rand::random_range(SHRIEKER_DEATH_SCREAM_DELAY_SECS),
                    },
                    Transform::from_translation(at),
                ));
                cues.write(SpecialCue {
                    cue: Cue::ShriekerDeath,
                    at: Some(at),
                });
            }
            SpecialKind::Sapper => {}
        }
    }
}

/// A death scream goes off after its delay, killing every Zombie in sight around it, Unpaid.
fn death_screams(
    mut commands: Commands,
    mut screams: Query<(Entity, &mut DeathScream, &Transform)>,
    zombies: Query<(Entity, &Transform), (With<Zombie>, Without<Dying>, Without<DeathScream>)>,
    spatial: SpatialQuery,
    mut damage: MessageWriter<DamageZombie>,
    mut cues: MessageWriter<SpecialCue>,
    time: Res<Time>,
) {
    for (entity, mut scream, transform) in &mut screams {
        scream.left -= time.delta_secs();
        if scream.left > 0.0 {
            continue;
        }
        let at = transform.translation;
        for (zombie, zombie_at) in &zombies {
            let target = zombie_at.translation;
            if target.distance(at) <= SHRIEKER_DEATH_SCREAM_RADIUS && in_sight(&spatial, at, target) {
                damage.write(DamageZombie {
                    player: scream.killer,
                    zombie,
                    damage: f32::INFINITY,
                    zone: HitZone::Head,
                    attack: Attack::Scream,
                });
                cues.write(SpecialCue {
                    cue: Cue::ScreamPop,
                    at: Some(target),
                });
            }
        }
        commands.entity(entity).despawn();
    }
}

/// Fire pools burn the Player standing in them every tick, and set alight the Zombies that walk
/// in, a few at a time, until they burn out.
fn fire_pools(
    mut commands: Commands,
    mut pools: Query<(Entity, &mut FirePool, &Transform)>,
    players: Players,
    zombies: Query<
        (Entity, &Transform, &Zombie),
        (Without<Dying>, Without<PoolBurning>, Without<Rising>, Without<FirePool>),
    >,
    burning: Query<(), (With<PoolBurning>, Without<Dying>)>,
    assets: Res<SpecialAssets>,
    mut hits: MessageWriter<PlayerHit>,
    mut cues: MessageWriter<SpecialCue>,
    time: Res<Time>,
) {
    let mut alight = burning.iter().count();
    for (entity, mut pool, transform) in &mut pools {
        let at = transform.translation;
        let due = pool.ticks.advance(time.delta_secs());
        if due > 0.0 {
            for player in players.iter().filter(|player| in_fire(at, player.feet)) {
                // Underfoot: no direction to show.
                hits.write(PlayerHit {
                    player: player.entity,
                    damage: due,
                    from: None,
                });
                if player.local {
                    cues.write(SpecialCue {
                        cue: Cue::FireBurn,
                        at: None,
                    });
                }
            }
        }
        for (zombie, zombie_at, body) in &zombies {
            if alight >= NAPALM_POOL_MAX_BURNING {
                break;
            }
            let zombie_feet = zombie_at.translation - Vec3::Y * 0.5;
            if body.special_kind().is_none() && in_fire(at, zombie_feet) {
                alight += 1;
                commands
                    .entity(zombie)
                    .insert(PoolBurning {
                        killer: pool.killer,
                        left: NAPALM_POOL_BURN_SECS,
                    })
                    .with_child((
                        Mesh3d(assets.flame.clone()),
                        MeshMaterial3d(assets.fire.clone()),
                        Transform::from_xyz(0.0, 0.2, 0.0),
                    ));
            }
        }
        if pool.ticks.done() {
            commands.entity(entity).despawn();
        }
    }
}

/// A Zombie alight from a fire pool dies once it has burned, Unpaid.
fn pool_burns(
    mut burning: Query<(Entity, &mut PoolBurning), Without<Dying>>,
    mut damage: MessageWriter<DamageZombie>,
    time: Res<Time>,
) {
    for (zombie, mut burn) in &mut burning {
        let before = burn.left;
        burn.left -= time.delta_secs();
        if before > 0.0 && burn.left <= 0.0 {
            damage.write(DamageZombie {
                player: burn.killer,
                zombie,
                damage: f32::INFINITY,
                zone: HitZone::Torso,
                attack: Attack::Napalm,
            });
        }
    }
}

/// The heat tint shows near a living Napalm, stronger the closer it is.
fn heat_tint(
    napalms: Query<&Transform, (With<Napalm>, Without<Dying>)>,
    player: Single<&Transform, (With<LocalPlayer>, Without<Napalm>)>,
    mut overlays: ResMut<Overlays>,
) {
    let nearest = napalms
        .iter()
        .map(|at| at.translation.distance(player.translation))
        .min_by(f32::total_cmp);
    if let Some(distance) = nearest {
        overlays.hold(Overlay::NapalmHeat, heat_strength(distance));
    }
}

/// A scream shakes the Player's view side to side, dying away. Only the camera's sideways offset
/// moves, so the Last Stand's eye height is left alone.
fn shake_view(
    mut shake: ResMut<ScreamShake>,
    mut camera: Single<&mut Transform, With<PlayerCamera>>,
    time: Res<Time>,
) {
    if shake.left > 0.0 {
        shake.left = (shake.left - time.delta_secs()).max(0.0);
        let amplitude = SHRIEKER_SHAKE_AMPLITUDE * (shake.left / SHRIEKER_SHAKE_SECS).min(1.0);
        let jitter = || (rand::random::<f32>() * 2.0 - 1.0) * amplitude;
        camera.translation.x = jitter();
        camera.translation.z = jitter();
    } else if camera.translation.x != 0.0 || camera.translation.z != 0.0 {
        camera.translation.x = 0.0;
        camera.translation.z = 0.0;
    }
}

fn fade_fireballs(mut commands: Commands, mut fireballs: Query<(Entity, &mut Fireball, &mut Transform)>, time: Res<Time>) {
    for (entity, mut fireball, mut transform) in &mut fireballs {
        fireball.age += time.delta_secs();
        if fireball.age >= FIREBALL_SECS {
            commands.entity(entity).despawn();
        } else {
            let t = fireball.age / FIREBALL_SECS;
            let size = NAPALM_KILL_RADIUS * 0.5 * (t * std::f32::consts::PI).sin().max(0.05);
            transform.scale = Vec3::splat(size);
        }
    }
}

/// A special that's stuck, or lost far from the Player, is moved to a fresh Special Spawn Point
/// instead of being cleaned up, so it never leaves the Horde or its slot; so is one Out of Bounds,
/// at once. Moved in the physics step, like the chase, as a clean cut: the easing starts from the
/// new spot too.
fn unstick(
    mut specials: Query<
        (&mut SpecialWatch, &mut Transform, &mut TranslationEasingState, &mut Route),
        (Without<Dying>, Without<Rising>, Without<Player>),
    >,
    players: Players,
    spawn_points: Query<(&Transform, &SpawnPoint), (Without<SpecialWatch>, Without<Player>)>,
    active: Res<ActiveZones>,
    map: Res<MapId>,
    time: Res<Time>,
) {
    for (mut watch, mut transform, mut easing, mut route) in &mut specials {
        let Some(player) = players.nearest(transform.translation) else {
            continue;
        };
        let stuck = watch.0.update(transform.translation, player.centre, time.delta_secs());
        if !stuck && !map.layout().out_of_bounds(transform.translation) {
            continue;
        }
        let points = special_spawn_candidates(
            spawn_points
                .iter()
                .filter(|(_, point)| point.kind == SpawnKind::Special)
                .map(|(t, point)| (point.zone, t.translation)),
            &players.spawn_anchors().iter().map(|spot| (spot.zone, spot.centre)).collect::<Vec<_>>(),
            active.0,
        );
        if !points.is_empty() {
            let at = points[rand::random_range(0..points.len())];
            transform.translation = at + Vec3::Y * (ZOMBIE_RADIUS + ZOMBIE_CAPSULE_LENGTH / 2.0 + 0.02);
            easing.start = Some(transform.translation);
            *route = Route::default();
        }
        watch.0 = StuckWatch::new(transform.translation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The specials a slot brings through Round 30, every gap drawn as `gap`, with each special
    /// killed in the Round it comes.
    fn specials(napalm: u32, shrieker: u32, gap: u32) -> Vec<(u32, SpecialKind)> {
        specials_with(napalm, shrieker, gap, &SpecialZombies::default())
    }

    /// `specials`, with the types switched as `on`.
    fn specials_with(napalm: u32, shrieker: u32, gap: u32, on: &SpecialZombies) -> Vec<(u32, SpecialKind)> {
        let mut slot = SpecialSlot::new(napalm, shrieker);
        let mut seen = Vec::new();
        for round in 1..=30 {
            if let Some(kind) = slot.due(round, false, false, on) {
                seen.push((round, kind));
                slot.died(kind, round, gap);
            }
        }
        seen
    }

    #[test]
    fn the_shrieker_comes_first_from_round_four_to_seven_and_the_napalm_from_five_to_seven() {
        use SpecialKind::*;
        assert_eq!(specials(6, 4, 3)[..2], [(4, Shrieker), (6, Napalm)]);
        assert_eq!(NAPALM_FIRST_ROUNDS, 5..=7);
        assert_eq!(SHRIEKER_FIRST_ROUNDS, 4..=7);
        assert_eq!(specials(5, 7, 9)[0], (5, Napalm));
        assert_eq!(SpecialSlot::new(5, 4).due(3, false, false, &SpecialZombies::default()), None);
    }

    #[test]
    fn at_most_one_special_a_round_the_napalm_checked_first() {
        use SpecialKind::*;
        let slot = SpecialSlot::new(6, 6);
        assert_eq!(slot.due(6, false, false, &SpecialZombies::default()), Some(Napalm));
        // The Shrieker waits until the Napalm isn't due.
        assert_eq!(specials(6, 6, 2)[..4], [(6, Napalm), (7, Shrieker), (8, Napalm), (9, Shrieker)]);
    }

    #[test]
    fn each_counter_moves_on_after_its_own_death() {
        use SpecialKind::*;
        let mut slot = SpecialSlot::new(5, 30);
        slot.died(Napalm, 5, 2);
        assert_eq!(slot.due(6, false, false, &SpecialZombies::default()), None);
        assert_eq!(slot.due(7, false, false, &SpecialZombies::default()), Some(Napalm));
        // A special that doesn't die stays due.
        assert_eq!(slot.due(8, false, false, &SpecialZombies::default()), Some(Napalm));
        slot.died(Napalm, 9, 1);
        assert_eq!(slot.due(10, false, false, &SpecialZombies::default()), Some(Napalm));
        assert_eq!(NAPALM_ROUND_GAPS, 1..=2);
        assert_eq!(SHRIEKER_ROUND_GAPS, 1..=3);
    }

    #[test]
    fn napalm_only_skips_the_shrieker() {
        let on = SpecialZombies { shrieker: false, ..default() };
        let seen = specials_with(6, 4, 2, &on);
        assert_eq!(seen[..3], [(6, SpecialKind::Napalm), (8, SpecialKind::Napalm), (10, SpecialKind::Napalm)]);
        assert!(seen.iter().all(|&(_, kind)| kind == SpecialKind::Napalm));
    }

    #[test]
    fn shrieker_only_skips_the_napalm_even_when_its_due_first() {
        let on = SpecialZombies { napalm: false, ..default() };
        let slot = SpecialSlot::new(5, 5);
        assert_eq!(slot.due(5, false, false, &on), Some(SpecialKind::Shrieker));
        let seen = specials_with(5, 6, 2, &on);
        assert_eq!(seen[..3], [(6, SpecialKind::Shrieker), (8, SpecialKind::Shrieker), (10, SpecialKind::Shrieker)]);
        assert!(seen.iter().all(|&(_, kind)| kind == SpecialKind::Shrieker));
    }

    #[test]
    fn with_both_off_the_slot_never_fills() {
        let on = SpecialZombies { napalm: false, shrieker: false, ..default() };
        assert!(specials_with(5, 4, 1, &on).is_empty());
    }

    #[test]
    fn every_special_kind_has_its_own_switch() {
        assert_eq!(SpecialKind::Napalm.switch(), SpecialType::Napalm);
        assert_eq!(SpecialKind::Shrieker.switch(), SpecialType::Shrieker);
        assert_eq!(SpecialKind::Sapper.switch(), SpecialType::Sapper);
    }

    #[test]
    fn no_special_on_a_hellhound_round_or_while_one_is_alive() {
        let slot = SpecialSlot::new(5, 4);
        assert_eq!(slot.due(6, true, false, &SpecialZombies::default()), None);
        assert_eq!(slot.due(6, false, true, &SpecialZombies::default()), None);
        assert_eq!(slot.due(7, false, false, &SpecialZombies::default()), Some(SpecialKind::Napalm));
    }

    #[test]
    fn a_special_spawns_a_quarter_to_three_quarters_into_the_horde() {
        assert_eq!(special_spawn_at(24, 0.25), 6);
        assert_eq!(special_spawn_at(24, 0.75), 18);
        assert_eq!(special_spawn_at(1, 0.75), 0, "always a Zombie left to take the place of");
        assert_eq!(special_spawn_at(0, 0.5), 0);
        assert_eq!(SPECIAL_SPAWN_HORDE_FRACTION, 0.25..=0.75);
    }

    #[test]
    fn specials_rise_in_the_players_zone_first_never_within_five_metres() {
        use ZoneId::*;
        let player = [(StartRoom, Vec3::ZERO)];
        let points = [
            (StartRoom, Vec3::new(3.0, 0.0, 0.0)),
            (StartRoom, Vec3::new(8.0, 0.0, 0.0)),
            (WestHall, Vec3::new(12.0, 0.0, 0.0)),
            (Yard, Vec3::new(20.0, 0.0, 0.0)),
        ];
        let active = ZoneSet::from_iter([StartRoom, WestHall]);
        assert_eq!(special_spawn_candidates(points, &player, active), [Vec3::new(8.0, 0.0, 0.0)]);
        // With no spot in the Player's Zone, any Active Zone's.
        let far = [(StartRoom, Vec3::new(3.0, 0.0, 0.0)), (WestHall, Vec3::new(12.0, 0.0, 0.0))];
        assert_eq!(special_spawn_candidates(far, &player, active), [Vec3::new(12.0, 0.0, 0.0)]);
        assert!(special_spawn_candidates([(StartRoom, Vec3::X)], &player, active).is_empty());
        // Two Players: never within five metres of either, in either's Zone first.
        let apart = [(StartRoom, Vec3::ZERO), (WestHall, Vec3::new(20.0, 0.0, 0.0))];
        assert_eq!(
            special_spawn_candidates(points, &apart, active),
            [Vec3::new(8.0, 0.0, 0.0), Vec3::new(12.0, 0.0, 0.0)]
        );
        let near = [(StartRoom, Vec3::ZERO), (WestHall, Vec3::new(10.0, 0.0, 0.0))];
        assert!(special_spawn_candidates(points, &near, active).is_empty());
    }

    #[test]
    fn napalm_blast_falls_off_250_100_then_100_to_50() {
        assert_eq!(napalm_blast_damage(0.0), Some(250.0));
        assert_eq!(napalm_blast_damage(2.29), Some(250.0));
        assert_eq!(napalm_blast_damage(2.3), Some(100.0));
        assert_eq!(napalm_blast_damage(3.8), Some(100.0));
        assert!((napalm_blast_damage(7.0).unwrap() - 75.0).abs() < 1e-3);
        assert!((napalm_blast_damage(10.2).unwrap() - 50.0).abs() < 1e-3);
        assert_eq!(napalm_blast_damage(10.3), None);
    }

    #[test]
    fn napalm_shellshock_lasts_three_seconds_close_to_one_and_a_half_at_the_edge() {
        assert_eq!(napalm_shellshock_secs(0.0), 3.0);
        assert!((napalm_shellshock_secs(5.1) - 2.25).abs() < 1e-5);
        assert_eq!(napalm_shellshock_secs(10.2), 1.5);
    }

    #[test]
    fn a_napalm_self_detonates_close_and_only_after_two_seconds() {
        assert!(napalm_triggers(Vec3::new(2.0, 1.0, 0.5), 2.0));
        assert!(!napalm_triggers(Vec3::new(2.0, 0.0, 0.0), 1.9), "too young");
        assert!(!napalm_triggers(Vec3::new(2.4, 0.0, 0.0), 5.0), "too far out");
        assert!(!napalm_triggers(Vec3::new(0.5, 1.4, 0.0), 5.0), "too far up");
    }

    #[test]
    fn napalm_points_pay_300_for_the_players_kill_only() {
        assert_eq!(napalm_kill_bonus(Attack::Bullet.pay()), 300);
        assert_eq!(napalm_kill_bonus(Attack::Napalm.pay()), 0, "a self-detonation pays nothing");
        assert!(!hit_pays(Some(SpecialKind::Napalm), 1));
        assert!(!hit_pays(Some(SpecialKind::Napalm), 2));
    }

    #[test]
    fn a_shrieker_pays_every_second_hit() {
        let paid: Vec<_> = (1..=6).map(|n| hit_pays(Some(SpecialKind::Shrieker), n)).collect();
        assert_eq!(paid, [false, true, false, true, false, true]);
        assert!(hit_pays(None, 1));
    }

    #[test]
    fn a_scream_reaches_only_in_front_and_in_range() {
        let forward = Vec3::NEG_Z;
        assert!(scream_reaches(Vec3::ZERO, forward, Vec3::new(0.0, 1.0, -7.0)));
        assert!(!scream_reaches(Vec3::ZERO, forward, Vec3::new(0.0, 0.0, -7.7)), "too far");
        assert!(!scream_reaches(Vec3::ZERO, forward, Vec3::new(0.0, 2.0, -3.0)), "too far up");
        assert!(!scream_reaches(Vec3::ZERO, forward, Vec3::new(0.0, 0.0, 3.0)), "behind it");
        // dot ≥ 0.4: about 66° either side.
        assert!(scream_reaches(Vec3::ZERO, forward, Vec3::new(4.0, 0.0, -2.0)));
        assert!(!scream_reaches(Vec3::ZERO, forward, Vec3::new(4.0, 0.0, -1.0)));
    }

    #[test]
    fn heat_tints_within_ten_metres_and_fire_pools_are_two_metres_wide() {
        assert_eq!(heat_strength(0.0), 1.0);
        assert_eq!(heat_strength(10.2), 0.0);
        assert_eq!(heat_strength(20.0), 0.0);
        assert!(in_fire(Vec3::ZERO, Vec3::new(1.9, 0.5, 0.0)));
        assert!(!in_fire(Vec3::ZERO, Vec3::new(2.1, 0.0, 0.0)));
        assert!(!in_fire(Vec3::ZERO, Vec3::new(0.0, 4.0, 0.0)), "a floor up");
    }

    #[test]
    fn specials_are_tougher_than_the_rounds_zombies() {
        assert_eq!(SpecialKind::Napalm.health(150.0), 600.0);
        assert_eq!(SpecialKind::Shrieker.health(150.0), 375.0);
        assert_eq!((SpecialKind::Napalm.rise_secs(), SpecialKind::Shrieker.rise_secs()), (1.5, 1.0));
        assert_eq!(SpecialKind::Sapper.health(150.0), 1200.0);
    }

    /// Every Special Spawn Point on the Map.
    fn special_spawn_points(world: &mut World) -> Vec<Vec3> {
        world
            .query::<(&Transform, &SpawnPoint)>()
            .iter(world)
            .filter(|(_, point)| point.kind == SpawnKind::Special)
            .map(|(t, _)| t.translation)
            .collect()
    }

    /// Moving a stuck special from `Update` was undone whenever the next frame ran a physics step:
    /// it snapped straight back. It lands on a Special Spawn Point on every frame offset and frame
    /// rate, with no frame partway there.
    #[test]
    fn a_stuck_special_moves_to_a_spawn_point_cleanly() {
        use crate::enemies::zombie::headless::Sim;
        for fps in [144.0, 60.0, 90.0] {
            for offset in 0..3 {
                let mut sim = Sim::new(fps, |app| {
                    app.insert_resource(ActiveZones(ZoneId::ALL.into_iter().collect()))
                        .add_systems(FixedUpdate, unstick);
                });
                let points = special_spawn_points(sim.world_mut());
                let on_a_point = |at: Vec3| points.iter().any(|p| p.with_y(0.0).distance(at.with_y(0.0)) < 0.5);
                let zombie = sim.spawn(Vec3::new(0.0, 0.0, 12.0), None);
                sim.run(5 + offset);
                let from = sim.frame(zombie).pos;
                sim.world_mut().entity_mut(zombie).insert(SpecialWatch(StuckWatch::nearly_up(from)));
                let samples = sim.track(zombie, sim.frames(0.3));
                for s in &samples {
                    let near_start = s.pos.with_y(0.0).distance(from.with_y(0.0)) < 0.5;
                    assert!(near_start || on_a_point(s.pos), "{fps} fps, offset {offset}: a frame at {}", s.pos);
                }
                let last = samples.last().unwrap().pos;
                assert!(on_a_point(last) && last.distance(from) > 2.0, "{fps} fps, offset {offset}: ended at {last}");
            }
        }
    }

    /// The Napalm's death blast measured against a Crouched Player: twice, once with the Player
    /// Crouched behind knee-high cover, once stood up behind the same cover.
    #[test]
    fn a_napalms_blast_is_measured_to_the_players_real_lowered_centre() {
        use crate::layers::GameLayer;
        use crate::players::player::two_players::{place, two_players_with};
        use crate::players::player::{Locomotion, MoveInput, Stance};
        use crate::enemies::zombie::headless::Sim;
        // Just the Napalm's death blast, without the rest of the special slot.
        let (mut sim, local, teammate) = two_players_with(|app| {
            app.insert_resource(SpecialSlot::new(*NAPALM_FIRST_ROUNDS.start(), *SHRIEKER_FIRST_ROUNDS.start()))
                .add_message::<Shellshock>()
                .add_systems(Startup, load_assets)
                .add_systems(Update, special_deaths);
        });
        // The local Player stands well clear, so the blast is the teammate's alone.
        place(&mut sim, local, Vec3::new(10.0, 0.0, 24.0));
        // Knee-high cover between the teammate and where the Napalm dies.
        sim.world_mut().spawn((
            RigidBody::Static,
            Collider::cuboid(0.1, 0.75, 4.0),
            Transform::from_xyz(-3.6, 0.375, 20.0),
            CollisionLayers::new(GameLayer::World, LayerMask::ALL),
        ));
        sim.world_mut().get_mut::<MoveInput>(teammate).unwrap().crouch = true;
        sim.run(10);
        assert_eq!(sim.world_mut().get::<Locomotion>(teammate).unwrap().stance, Stance::Crouched);

        // A Napalm dies on the cover's far side, and every Player its blast reaches is hit.
        let blast = |sim: &mut Sim| {
            let napalm = sim.spawn(Vec3::new(-5.2, 0.0, 20.0), None);
            sim.world_mut().get_mut::<Zombie>(napalm).unwrap().become_special(SpecialKind::Napalm);
            sim.world_mut().entity_mut(napalm).insert(Dying::default());
            let mut hit = Vec::new();
            for _ in 0..4 {
                sim.run(1);
                hit.extend(sim.world_mut().resource_mut::<Messages<PlayerHit>>().drain().map(|hit| hit.player));
            }
            hit
        };
        // Crouched, the blast measures the real lowered centre, and the cover shields it.
        assert!(blast(&mut sim).is_empty(), "the low cover didn't shield the Crouched Player");
        // Standing back up, the same blast reaches them.
        sim.world_mut().get_mut::<MoveInput>(teammate).unwrap().crouch = false;
        sim.run(10);
        assert_eq!(blast(&mut sim), [teammate], "the blast missed the standing Player");
    }
}
