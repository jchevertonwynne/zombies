//! Ammo Mods: what a tap Repack adds to an Upgraded Weapon. Each one a Weapon holds rolls on its
//! own once per damage event its Weapon deals (bullet, pellet, grenade, projectile, explosive or
//! impact); a trigger starts that Ammo Mod's cooldown. Melee, burn and Ammo Mod damage never roll.
//!
//! - Blast Furnace kills the shot Zombie and sets alight everything near it.
//! - Dead Wire kills the shot Zombie and chains to up to 8 more.
//! - Fireworks kills the shot Zombie and raises a phantom gun that kills 10 more (Unpaid).
//! - Thunder Wall kills and flings up to 6 in front of the Player.
//! - Turned makes the shot Zombie an unkillable ally for a while (all Unpaid).

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::arsenal::Burn;
use crate::burn::{Burning, Ignite};
use crate::door::Door;
use crate::explosive::{in_sight, world_filter};
use crate::layers::GameLayer;
use crate::hellhound::Telegraph;
use crate::modifier::{Modifiers, Stat, resolve};
use crate::barrier::Barriers;
use crate::nav::{NavGraph, Route};
use crate::player::{Player, PlayerCamera};
use crate::state::{GameState, InRun, Pause};
use crate::tuning::*;
use crate::zombie::{
    Attack, DamageZombie, Dying, HitArea, HitZone, Lured, Pay, Rising, Zombie, ZombieKilled, apply_damage,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmmoMod {
    BlastFurnace,
    DeadWire,
    Fireworks,
    ThunderWall,
    Turned,
}

impl AmmoMod {
    pub const ALL: [AmmoMod; 5] = [
        AmmoMod::BlastFurnace,
        AmmoMod::DeadWire,
        AmmoMod::Fireworks,
        AmmoMod::ThunderWall,
        AmmoMod::Turned,
    ];

    fn index(self) -> usize {
        self as usize
    }

    #[allow(dead_code, reason = "for the HUD and debug output")]
    pub fn name(self) -> &'static str {
        match self {
            AmmoMod::BlastFurnace => "Blast Furnace",
            AmmoMod::DeadWire => "Dead Wire",
            AmmoMod::Fireworks => "Fireworks",
            AmmoMod::ThunderWall => "Thunder Wall",
            AmmoMod::Turned => "Turned",
        }
    }

    /// Its chance to trigger on one damage event.
    pub fn chance(self) -> f32 {
        match self {
            AmmoMod::BlastFurnace => BLAST_FURNACE_CHANCE,
            AmmoMod::DeadWire => DEAD_WIRE_CHANCE,
            AmmoMod::Fireworks => FIREWORKS_CHANCE,
            AmmoMod::ThunderWall => THUNDER_WALL_CHANCE,
            AmmoMod::Turned => TURNED_CHANCE,
        }
    }

    /// The cooldown a trigger starts, before any modifier.
    pub fn cooldown_secs(self) -> f32 {
        match self {
            AmmoMod::BlastFurnace => BLAST_FURNACE_COOLDOWN_SECS,
            AmmoMod::DeadWire => DEAD_WIRE_COOLDOWN_SECS,
            AmmoMod::Fireworks => FIREWORKS_COOLDOWN_SECS,
            AmmoMod::ThunderWall => THUNDER_WALL_COOLDOWN_SECS,
            AmmoMod::Turned => TURNED_COOLDOWN_SECS,
        }
    }

    /// Whether it can trigger on a hit that kills. Turned needs a Zombie left to turn.
    fn on_killing_hit(self) -> bool {
        self != AmmoMod::Turned
    }

    /// Whether it can trigger on a Zombie still climbing out of the ground.
    fn on_emerging(self) -> bool {
        !matches!(self, AmmoMod::Fireworks | AmmoMod::Turned)
    }

    /// Its colour in the glint.
    pub fn glint(self) -> Color {
        match self {
            AmmoMod::BlastFurnace => Color::srgb(1.0, 0.45, 0.05),
            AmmoMod::DeadWire => Color::srgb(0.1, 0.9, 1.0),
            AmmoMod::Fireworks => Color::srgb(1.0, 0.8, 0.15),
            AmmoMod::ThunderWall => Color::srgb(1.0, 1.0, 1.0),
            AmmoMod::Turned => Color::srgb(0.2, 1.0, 0.25),
        }
    }
}

/// The Ammo Mods one Weapon copy holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AmmoMods(u8);

impl AmmoMods {
    pub const NONE: AmmoMods = AmmoMods(0);

    pub fn contains(self, ammo_mod: AmmoMod) -> bool {
        self.0 & (1 << ammo_mod.index()) != 0
    }

    pub fn with(self, ammo_mod: AmmoMod) -> Self {
        Self(self.0 | 1 << ammo_mod.index())
    }

    pub fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether a tap Repack can add another.
    pub fn has_room(self) -> bool {
        self.len() < MAX_AMMO_MODS
    }

    pub fn iter(self) -> impl Iterator<Item = AmmoMod> {
        AmmoMod::ALL.into_iter().filter(move |ammo_mod| self.contains(*ammo_mod))
    }
}

/// The price of a tap Repack of a copy already Repacked `repacks` times (of either kind).
pub fn repack_price(repacks: u32) -> u64 {
    REPACK_AMMO_MOD_PRICE.saturating_add(REPACK_PRICE_STEP.saturating_mul(repacks as u64))
}

/// The Ammo Mod a tap Repack adds: one the Weapon lacks, picked uniformly by `roll` in [0, 1).
/// `None` once it holds every one.
pub fn pick_ammo_mod(held: AmmoMods, roll: f32) -> Option<AmmoMod> {
    let lacking: Vec<_> = AmmoMod::ALL.into_iter().filter(|ammo_mod| !held.contains(*ammo_mod)).collect();
    if lacking.is_empty() {
        return None;
    }
    let i = ((roll * lacking.len() as f32) as usize).min(lacking.len() - 1);
    Some(lacking[i])
}

/// The glint's colour on its `pass`th sweep: purple with no Ammo Mod, the Ammo Mod's colour with
/// one, and each held Ammo Mod's colour in turn with more.
pub fn glint_colour(held: AmmoMods, pass: u32, plain: Color) -> Color {
    let colours: Vec<_> = held.iter().map(AmmoMod::glint).collect();
    if colours.is_empty() { plain } else { colours[pass as usize % colours.len()] }
}

/// Seconds left on each Ammo Mod's cooldown. Solo play has one Player, so a cooldown is shared by
/// every Weapon holding that Ammo Mod.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct Cooldowns([f32; 5]);

impl Cooldowns {
    pub fn ready(&self, ammo_mod: AmmoMod) -> bool {
        self.0[ammo_mod.index()] <= 0.0
    }

    pub fn start(&mut self, ammo_mod: AmmoMod, secs: f32) {
        self.0[ammo_mod.index()] = secs;
    }

    pub fn tick(&mut self, dt: f32) {
        for left in &mut self.0 {
            *left = (*left - dt).max(0.0);
        }
    }
}

/// The Zombie a damage event landed on, as the roll sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Struck {
    /// Whether the hit kills it (or it was dead already).
    pub lethal: bool,
    /// Whether it's still climbing out of the ground.
    pub emerging: bool,
}

/// The held Ammo Mods that trigger on one damage event. Each ready one rolls on its own, taking
/// one number in [0, 1) from `roll`, so two can trigger on the same hit. Every trigger but
/// Turned's kills the shot Zombie, so Turned rolls last and never on a hit another one kills.
pub fn triggers(held: AmmoMods, cooldowns: &Cooldowns, struck: Struck, mut roll: impl FnMut() -> f32) -> Vec<AmmoMod> {
    let mut fired = Vec::new();
    let mut lethal = struck.lethal;
    for ammo_mod in held.iter() {
        if !cooldowns.ready(ammo_mod)
            || (lethal && !ammo_mod.on_killing_hit())
            || (struck.emerging && !ammo_mod.on_emerging())
        {
            continue;
        }
        if roll() < ammo_mod.chance() {
            fired.push(ammo_mod);
            lethal = true;
        }
    }
    fired
}

/// One link of a Dead Wire chain: Zombie `index` in the list given, hit from `from` (`None` for
/// the shot Zombie) at jump `depth`, counting from 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    pub index: usize,
    pub from: Option<usize>,
    pub depth: u32,
}

/// Dead Wire's chain out from the shot Zombie at `shot` through the others at `others`. Each
/// Zombie hit arcs on to every Zombie not yet hit within its jump's radius, given a clear line,
/// until the radii run out or `DEAD_WIRE_MAX_ZOMBIES` (the shot one included) are hit.
pub fn dead_wire_chain(shot: Vec3, others: &[Vec3], clear: impl Fn(Vec3, Vec3) -> bool) -> Vec<Link> {
    let mut links: Vec<Link> = Vec::new();
    let mut hit = vec![false; others.len()];
    // Breadth first: (index or the shot, where it stands, its depth).
    let mut queue = std::collections::VecDeque::from([(None, shot, 0u32)]);
    while let Some((from, at, depth)) = queue.pop_front() {
        let Some(&radius) = DEAD_WIRE_JUMP_RADII.get(depth as usize) else { continue };
        for (index, &other) in others.iter().enumerate() {
            if links.len() + 1 >= DEAD_WIRE_MAX_ZOMBIES {
                return links;
            }
            if hit[index] || at.distance(other) > radius || !clear(at, other) {
                continue;
            }
            hit[index] = true;
            links.push(Link { index, from, depth: depth + 1 });
            queue.push_back((Some(index), other, depth + 1));
        }
    }
    links
}

/// The other Zombies (by index) a Thunder Wall at `blast` kills: those within its radius and in
/// front of the Player's `forward`, closest first, up to the cap less the shot Zombie.
pub fn thunder_wall_targets(blast: Vec3, forward: Vec3, others: &[Vec3]) -> Vec<usize> {
    closest_within(blast, others, THUNDER_WALL_RADIUS, THUNDER_WALL_MAX_ZOMBIES - 1, |at| {
        (at - blast).dot(forward) >= 0.0
    })
}

/// The other Zombies (by index) a turn kills: the closest within its blast radius, up to its cap.
pub fn turn_blast_targets(at: Vec3, others: &[Vec3]) -> Vec<usize> {
    closest_within(at, others, TURNED_BLAST_RADIUS, TURNED_BLAST_MAX_ZOMBIES, |_| true)
}

fn closest_within(centre: Vec3, others: &[Vec3], radius: f32, cap: usize, keep: impl Fn(Vec3) -> bool) -> Vec<usize> {
    let mut near: Vec<_> = (0..others.len())
        .filter(|&i| centre.distance(others[i]) <= radius && keep(others[i]))
        .collect();
    near.sort_by(|&a, &b| centre.distance(others[a]).total_cmp(&centre.distance(others[b])));
    near.truncate(cap);
    near
}

/// Which candidate a phantom gun's shot aims at. Candidates come shuffled, each with its
/// distance; the first within range wins, but only the first `FIREWORKS_SIGHT_CHECKS` in range
/// need a clear line, after which the next in range is taken regardless.
pub fn fireworks_target(candidates: &[(usize, f32)], mut clear: impl FnMut(usize) -> bool) -> Option<usize> {
    let mut checked = 0;
    for &(index, distance) in candidates {
        if distance > FIREWORKS_RANGE {
            continue;
        }
        if checked >= FIREWORKS_SIGHT_CHECKS {
            return Some(index);
        }
        checked += 1;
        if clear(index) {
            return Some(index);
        }
    }
    None
}

/// The per-tick damage of a Blast Furnace burn on a Zombie with `health`: it dies on the last tick.
pub fn blast_furnace_burn(health: f32) -> Burn {
    let ticks = BLAST_FURNACE_BURN_TICKS as f32;
    Burn {
        // A hair over, so rounding never leaves it standing.
        damage: health / ticks * 1.001,
        tick_secs: BLAST_FURNACE_TICK_SECS,
        secs: BLAST_FURNACE_TICK_SECS * ticks,
    }
}

/// A damage event from a Weapon holding Ammo Mods, written beside its `DamageZombie`.
#[derive(Message, Debug, Clone, Copy)]
pub struct AmmoModHit {
    pub zombie: Entity,
    pub damage: f32,
    pub ammo_mods: AmmoMods,
}

/// A Zombie on the Player's side after a Turned trigger. It's no longer a `Zombie`, so nothing
/// can hurt it and the Round doesn't wait for it.
#[derive(Component, Debug, Default)]
pub struct TurnedAlly {
    elapsed: f32,
    kills: u32,
    wind_up: Option<f32>,
    /// Seconds left of its last swing once its time or kills are up.
    ending: Option<f32>,
    fall_speed: f32,
}

/// A body thrown by a Thunder Wall or a turn.
#[derive(Component, Debug)]
struct Flung {
    velocity: Vec3,
    floor: f32,
}

/// The phantom gun Fireworks raises where the shot Zombie fell.
#[derive(Component, Debug)]
struct Phantom {
    base: Vec3,
    elapsed: f32,
    shots: u32,
}

/// Dead Wire hits waiting out their delay: (seconds left, Zombie).
#[derive(Resource, Default)]
struct DelayedHits(Vec<(f32, Entity)>);

/// Short-lived lines and bursts showing each Ammo Mod at work.
#[derive(Resource, Default)]
struct Flashes(Vec<Flash>);

struct Flash {
    shape: Shape,
    colour: Color,
    life: f32,
}

enum Shape {
    Line(Vec3, Vec3),
    Burst(Vec3, f32),
}

const FLASH_SECS: f32 = 0.25;

#[derive(Resource)]
struct AmmoModAssets {
    ally: Handle<StandardMaterial>,
    phantom: Handle<StandardMaterial>,
    gun: Handle<Mesh>,
}

pub struct AmmoModPlugin;

impl Plugin for AmmoModPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AmmoModHit>()
            .init_resource::<Cooldowns>()
            .init_resource::<DelayedHits>()
            .init_resource::<Flashes>()
            .add_systems(Startup, load_assets)
            .add_systems(OnEnter(GameState::Playing), reset)
            .add_systems(
                Update,
                (
                    (roll_ammo_mods, run_delayed_hits, fire_phantoms, run_allies)
                        .chain()
                        .before(apply_damage),
                    (fling, draw_flashes),
                )
                    .run_if(in_state(Pause::Running)),
            );
    }
}

fn load_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let glow = |colour: Color| StandardMaterial {
        base_color: colour,
        emissive: LinearRgba::from(colour) * 4.0,
        ..default()
    };
    commands.insert_resource(AmmoModAssets {
        ally: materials.add(glow(AmmoMod::Turned.glint())),
        phantom: materials.add(glow(AmmoMod::Fireworks.glint())),
        gun: meshes.add(Cuboid::new(0.08, 0.14, 0.6)),
    });
}

fn reset(mut cooldowns: ResMut<Cooldowns>, mut delayed: ResMut<DelayedHits>, mut flashes: ResMut<Flashes>) {
    *cooldowns = Cooldowns::default();
    delayed.0.clear();
    flashes.0.clear();
}

/// Enough damage to kill any Zombie outright.
const KILL: f32 = f32::INFINITY;

fn kill(damage: &mut MessageWriter<DamageZombie>, zombie: Entity, pay: Pay) {
    damage.write(DamageZombie {
        zombie,
        damage: KILL,
        zone: HitZone::Torso,
        attack: Attack::AmmoMod(pay),
    });
}

/// Each Weapon damage event rolls the Weapon's Ammo Mods, and every trigger takes effect.
fn roll_ammo_mods(
    mut commands: Commands,
    mut hits: MessageReader<AmmoModHit>,
    mut cooldowns: ResMut<Cooldowns>,
    mut delayed: ResMut<DelayedHits>,
    mut flashes: ResMut<Flashes>,
    // A hound still in its lightning strike can't be hurt, so no Ammo Mod touches it.
    zombies: Query<(Entity, &Zombie, &Transform, &Children, Has<Dying>, Has<Rising>), Without<Telegraph>>,
    hit_areas: Query<(), With<HitArea>>,
    player: Single<&Modifiers, With<Player>>,
    camera: Single<&GlobalTransform, With<PlayerCamera>>,
    spatial: SpatialQuery,
    assets: Res<AmmoModAssets>,
    mut damage: MessageWriter<DamageZombie>,
    mut ignite: MessageWriter<Ignite>,
    mut kills: MessageWriter<ZombieKilled>,
    time: Res<Time>,
) {
    cooldowns.tick(time.delta_secs());
    // Zombies already killed or turned by a trigger this frame.
    let mut spent: Vec<Entity> = Vec::new();
    for hit in hits.read() {
        let Ok((shot, zombie, transform, children, dying, rising)) = zombies.get(hit.zombie) else {
            continue;
        };
        if spent.contains(&shot) {
            continue;
        }
        let struck = Struck {
            lethal: dying || zombie.health() <= 0.0 || hit.damage >= zombie.health(),
            emerging: rising,
        };
        let fired = triggers(hit.ammo_mods, &cooldowns, struck, rand::random::<f32>);
        if fired.is_empty() {
            continue;
        }
        let at = transform.translation;
        // Every other living Zombie, with where it stands.
        let (others, positions): (Vec<Entity>, Vec<Vec3>) = zombies
            .iter()
            .filter(|(entity, _, _, _, dying, _)| *entity != shot && !dying && !spent.contains(entity))
            .map(|(entity, _, transform, ..)| (entity, transform.translation))
            .unzip();
        for ammo_mod in fired {
            cooldowns.start(ammo_mod, resolve(Stat::AmmoModCooldown, ammo_mod.cooldown_secs(), &player.0));
            let colour = ammo_mod.glint();
            match ammo_mod {
                AmmoMod::BlastFurnace => {
                    kill(&mut damage, shot, Pay::Paid);
                    for (i, &other) in positions.iter().enumerate() {
                        if at.distance(other) <= BLAST_FURNACE_RADIUS
                            && let Ok((_, target, ..)) = zombies.get(others[i])
                        {
                            ignite.write(Ignite {
                                zombie: others[i],
                                burn: blast_furnace_burn(target.health()),
                            });
                        }
                    }
                    flashes.burst(at, BLAST_FURNACE_RADIUS, colour);
                }
                AmmoMod::DeadWire => {
                    kill(&mut damage, shot, Pay::Paid);
                    let chain = dead_wire_chain(at, &positions, |a, b| in_sight(&spatial, a + Vec3::Y * 0.6, b + Vec3::Y * 0.6));
                    for link in chain {
                        let zombie = others[link.index];
                        let from = link.from.map_or(at, |from| positions[from]);
                        flashes.line(from, positions[link.index], colour);
                        let delay = if link.depth > 1 {
                            rand::random_range(DEAD_WIRE_DELAY_MIN_SECS..DEAD_WIRE_DELAY_MAX_SECS) * link.depth as f32
                        } else {
                            0.0
                        };
                        delayed.0.push((delay, zombie));
                        spent.push(zombie);
                    }
                }
                AmmoMod::Fireworks => {
                    kill(&mut damage, shot, Pay::Paid);
                    let feet = at - Vec3::Y * (ZOMBIE_RADIUS + ZOMBIE_CAPSULE_LENGTH / 2.0);
                    commands.spawn((
                        InRun,
                        Name::new("Fireworks phantom"),
                        Phantom {
                            base: feet,
                            elapsed: 0.0,
                            shots: 0,
                        },
                        Mesh3d(assets.gun.clone()),
                        MeshMaterial3d(assets.phantom.clone()),
                        Transform::from_translation(feet),
                    ));
                    flashes.burst(at, 0.6, colour);
                }
                AmmoMod::ThunderWall => {
                    kill(&mut damage, shot, Pay::Paid);
                    let forward = camera.forward().with_y(0.0).normalize_or_zero();
                    let targets = thunder_wall_targets(at, forward, &positions);
                    commands.entity(shot).insert(Flung::away(at - forward, at, THUNDER_WALL_FLING_SPEED, THUNDER_WALL_FLING_LIFT));
                    for i in targets {
                        kill(&mut damage, others[i], Pay::Paid);
                        commands.entity(others[i]).insert(Flung::away(at, positions[i], THUNDER_WALL_FLING_SPEED, THUNDER_WALL_FLING_LIFT));
                        spent.push(others[i]);
                    }
                    flashes.burst(at, THUNDER_WALL_RADIUS, colour);
                }
                AmmoMod::Turned => {
                    // The turn counts toward the Round as an Unpaid Kill; the ally lives on.
                    kills.write(ZombieKilled {
                        pay: Pay::Unpaid,
                        headshot: false,
                        zone: HitZone::Torso,
                        attack: Attack::AmmoMod(Pay::Unpaid),
                        at,
                    });
                    commands
                        .entity(shot)
                        .remove::<(Zombie, Lured, Burning)>()
                        .insert((TurnedAlly::default(), MeshMaterial3d(assets.ally.clone())));
                    // Bullets pass through it.
                    for child in children.iter() {
                        if hit_areas.contains(child) {
                            commands.entity(child).remove::<(Collider, CollisionLayers)>();
                        }
                    }
                    for i in turn_blast_targets(at, &positions) {
                        kill(&mut damage, others[i], Pay::Unpaid);
                        commands.entity(others[i]).insert(Flung::away(at, positions[i], TURNED_FLING_SPEED, TURNED_FLING_LIFT));
                        spent.push(others[i]);
                    }
                    flashes.burst(at, TURNED_BLAST_RADIUS, colour);
                }
            }
        }
        spent.push(shot);
    }
}

/// Dead Wire hits land once their delay is up, each dealing the Zombie's health and then some.
fn run_delayed_hits(
    mut delayed: ResMut<DelayedHits>,
    zombies: Query<&Zombie, Without<Dying>>,
    mut damage: MessageWriter<DamageZombie>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    delayed.0.retain_mut(|(left, zombie)| {
        *left -= dt;
        if *left > 0.0 {
            return true;
        }
        if let Ok(target) = zombies.get(*zombie) {
            damage.write(DamageZombie {
                zombie: *zombie,
                damage: target.health() + DEAD_WIRE_EXTRA_DAMAGE,
                zone: HitZone::Torso,
                attack: Attack::AmmoMod(Pay::Paid),
            });
        }
        false
    });
}

/// The phantom gun rises, fires its shots at random Zombies, each a kill (Unpaid), then sinks away.
fn fire_phantoms(
    mut commands: Commands,
    mut phantoms: Query<(Entity, &mut Phantom, &mut Transform)>,
    zombies: Query<(Entity, &GlobalTransform), (With<Zombie>, Without<Dying>)>,
    mut flashes: ResMut<Flashes>,
    spatial: SpatialQuery,
    mut damage: MessageWriter<DamageZombie>,
    time: Res<Time>,
) {
    let mut doomed: Vec<Entity> = Vec::new();
    for (entity, mut phantom, mut transform) in &mut phantoms {
        phantom.elapsed += time.delta_secs();
        let t = phantom.elapsed;
        let firing_ends = FIREWORKS_RISE_SECS + FIREWORKS_SHOTS as f32 * FIREWORKS_SHOT_SECS;
        let rise = if t < FIREWORKS_RISE_SECS {
            t / FIREWORKS_RISE_SECS
        } else if t < firing_ends {
            1.0
        } else {
            1.0 - (t - firing_ends) / FIREWORKS_RISE_SECS
        };
        if rise <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let muzzle = phantom.base + Vec3::Y * FIREWORKS_RISE * rise;
        transform.translation = muzzle;

        while phantom.shots < FIREWORKS_SHOTS
            && t >= FIREWORKS_RISE_SECS + phantom.shots as f32 * FIREWORKS_SHOT_SECS
        {
            phantom.shots += 1;
            let mut candidates: Vec<_> = zombies
                .iter()
                .filter(|(zombie, _)| !doomed.contains(zombie))
                .map(|(zombie, at)| (zombie, at.translation()))
                .collect();
            candidates.sort_by_cached_key(|_| rand::random::<u32>());
            let ranked: Vec<_> = candidates.iter().enumerate().map(|(i, (_, at))| (i, muzzle.distance(*at))).collect();
            let target = fireworks_target(&ranked, |i| in_sight(&spatial, muzzle, candidates[i].1));
            let aim = match target {
                Some(i) => {
                    let (zombie, at) = candidates[i];
                    kill(&mut damage, zombie, Pay::Unpaid);
                    doomed.push(zombie);
                    at
                }
                // Nothing in range: a shot off at a random heading.
                None => {
                    let heading = rand::random::<f32>() * std::f32::consts::TAU;
                    muzzle + Vec3::new(heading.cos(), 0.0, heading.sin()) * FIREWORKS_RANGE
                }
            };
            if let Ok(direction) = Dir3::new(aim - muzzle) {
                transform.rotation = Transform::IDENTITY.looking_to(direction, Vec3::Y).rotation;
            }
            flashes.line(muzzle, aim, AmmoMod::Fireworks.glint());
        }
    }
}

/// Turned allies sprint at the closest Zombie and kill it with one swipe (Unpaid), until their
/// time or kills run out; then they fall, with no Points for anyone.
fn run_allies(
    mut commands: Commands,
    mut allies: Query<(Entity, &mut TurnedAlly, &mut Route, &mut Transform, &Collider), Without<Zombie>>,
    zombies: Query<(Entity, &Transform), (With<Zombie>, Without<Dying>, Without<Rising>)>,
    graph: Res<NavGraph>,
    barriers: Res<Barriers>,
    doors: Query<&Door>,
    move_and_slide: MoveAndSlide,
    mut damage: MessageWriter<DamageZombie>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    let half = ZOMBIE_RADIUS + ZOMBIE_CAPSULE_LENGTH / 2.0;
    let open_doors: Vec<_> = doors.iter().filter(|door| door.open).filter_map(|door| door.joins).collect();
    let filter_mask = [GameLayer::World, GameLayer::Zombie, GameLayer::Player];
    for (entity, mut ally, mut route, mut transform, collider) in &mut allies {
        ally.elapsed += dt;
        if ally.ending.is_none() && (ally.elapsed >= TURNED_LIFETIME_SECS || ally.kills >= TURNED_MAX_KILLS) {
            ally.ending = Some(TURNED_LAST_SWING_SECS);
        }
        if let Some(left) = &mut ally.ending {
            *left -= dt;
            if *left <= 0.0 {
                commands
                    .entity(entity)
                    .remove::<(TurnedAlly, Collider, CollisionLayers)>()
                    .insert(Dying::default());
                continue;
            }
        }

        let feet = transform.translation - Vec3::Y * half;
        let target = zombies
            .iter()
            .min_by(|(_, a), (_, b)| {
                a.translation.distance(transform.translation).total_cmp(&b.translation.distance(transform.translation))
            })
            .map(|(zombie, at)| (zombie, at.translation));

        let mut stride = Vec3::ZERO;
        if let Some((zombie, at)) = target {
            let to_target = (at - transform.translation).with_y(0.0);
            let in_reach = to_target.length() <= ZOMBIE_REACH && (at.y - transform.translation.y).abs() < 1.5;
            match ally.wind_up {
                Some(left) if left - dt <= 0.0 => {
                    ally.wind_up = None;
                    if in_reach && ally.kills < TURNED_MAX_KILLS {
                        kill(&mut damage, zombie, Pay::Unpaid);
                        ally.kills += 1;
                    }
                }
                Some(left) => ally.wind_up = Some(left - dt),
                None if in_reach && ally.ending.is_none() => ally.wind_up = Some(ZOMBIE_WIND_UP_SECS),
                None => {}
            }

            route.repath_in -= dt;
            if route.repath_in <= 0.0 {
                route.repath_in = 0.5;
                let target_feet = at - Vec3::Y * half;
                let (from, to) = (graph.nearest(feet), graph.nearest(target_feet));
                route.waypoints = if from == to {
                    Vec::new()
                } else {
                    let path = graph.plan(from, to, &open_doors, |b| barriers.is_open(b)).unwrap_or_default();
                    path.into_iter().skip(1).map(|i| graph.nodes[i].at).collect()
                };
            }
            if let Some(&next) = route.waypoints.first()
                && (next - feet).with_y(0.0).length() < ZOMBIE_WAYPOINT_REACH
            {
                route.waypoints.remove(0);
            }
            let goal = route.waypoints.first().map_or(at - Vec3::Y * half, |next| *next);
            let heading = (goal - feet).with_y(0.0).normalize_or_zero();
            let facing = if route.waypoints.is_empty() { to_target } else { heading };
            if facing.length_squared() > 1e-4 {
                transform.rotation = Quat::from_rotation_y(f32::atan2(-facing.x, -facing.z));
            }
            if !in_reach && ally.wind_up.is_none() {
                stride = heading * ZOMBIE_SPRINT_SPEED;
            }
        }

        let velocity = stride + Vec3::Y * (ally.fall_speed - GRAVITY * dt);
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
        ally.fall_speed = if grounded { out.projected_velocity.y.min(0.0) } else { out.projected_velocity.y };
    }
}

impl Flung {
    /// Thrown from `from` away through `at`, with some lift.
    fn away(from: Vec3, at: Vec3, speed: f32, lift: f32) -> Self {
        let away = (at - from).with_y(0.0).normalize_or_zero();
        Self {
            velocity: away * speed + Vec3::Y * lift,
            floor: at.y,
        }
    }
}

/// Flung bodies fly until they land back at their height, stopped short by walls.
fn fling(
    mut commands: Commands,
    mut bodies: Query<(Entity, &mut Flung, &mut Transform)>,
    spatial: SpatialQuery,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    for (entity, mut flung, mut transform) in &mut bodies {
        let step = flung.velocity.with_y(0.0) * dt;
        if let Ok((direction, length)) = Dir3::new_and_length(step) {
            if spatial
                .cast_ray(transform.translation, direction, length + ZOMBIE_RADIUS, true, &world_filter())
                .is_some()
            {
                flung.velocity = flung.velocity.with_x(0.0).with_z(0.0);
            } else {
                transform.translation += step;
            }
        }
        transform.translation.y += flung.velocity.y * dt;
        flung.velocity.y -= GRAVITY * dt;
        if flung.velocity.y < 0.0 && transform.translation.y <= flung.floor {
            transform.translation.y = flung.floor;
            commands.entity(entity).remove::<Flung>();
        }
    }
}

impl Flashes {
    fn line(&mut self, from: Vec3, to: Vec3, colour: Color) {
        self.0.push(Flash {
            shape: Shape::Line(from, to),
            colour,
            life: FLASH_SECS,
        });
    }

    fn burst(&mut self, at: Vec3, radius: f32, colour: Color) {
        self.0.push(Flash {
            shape: Shape::Burst(at, radius),
            colour,
            life: FLASH_SECS,
        });
    }
}

fn draw_flashes(mut flashes: ResMut<Flashes>, mut gizmos: Gizmos, time: Res<Time>) {
    let dt = time.delta_secs();
    flashes.0.retain_mut(|flash| {
        flash.life -= dt;
        match flash.shape {
            Shape::Line(from, to) => {
                gizmos.line(from, to, flash.colour);
            }
            Shape::Burst(at, radius) => {
                // The burst grows out to its radius as it fades.
                let grown = radius * (1.0 - flash.life / FLASH_SECS).clamp(0.1, 1.0);
                gizmos.sphere(Isometry3d::from_translation(at), grown, flash.colour);
            }
        }
        flash.life > 0.0
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALIVE: Struck = Struck {
        lethal: false,
        emerging: false,
    };

    fn held(mods: &[AmmoMod]) -> AmmoMods {
        mods.iter().fold(AmmoMods::NONE, |held, m| held.with(*m))
    }

    /// Rolls that come out as given, in order.
    fn rolls(values: &[f32]) -> impl FnMut() -> f32 + '_ {
        let mut next = values.iter();
        move || *next.next().expect("no more rolls")
    }

    #[test]
    fn each_repack_of_a_copy_costs_1000_more() {
        assert_eq!(repack_price(0), 2500);
        assert_eq!(repack_price(1), 3500);
        assert_eq!(repack_price(3), 5500);
        assert_eq!(repack_price(u32::MAX), REPACK_AMMO_MOD_PRICE + REPACK_PRICE_STEP * u32::MAX as u64);
    }

    #[test]
    fn a_weapon_holds_at_most_two_ammo_mods() {
        assert!(AmmoMods::NONE.has_room());
        assert!(held(&[AmmoMod::Turned]).has_room());
        let full = held(&[AmmoMod::Turned, AmmoMod::DeadWire]);
        assert_eq!(full.len(), 2);
        assert!(!full.has_room());
    }

    #[test]
    fn a_repack_adds_an_ammo_mod_the_weapon_lacks() {
        let have = held(&[AmmoMod::BlastFurnace, AmmoMod::Fireworks]);
        for i in 0..100 {
            let pick = pick_ammo_mod(have, i as f32 / 100.0).unwrap();
            assert!(!have.contains(pick), "picked {pick:?} again");
        }
        // Uniform over the three lacking.
        assert_eq!(pick_ammo_mod(have, 0.0), Some(AmmoMod::DeadWire));
        assert_eq!(pick_ammo_mod(have, 0.5), Some(AmmoMod::ThunderWall));
        assert_eq!(pick_ammo_mod(have, 0.99), Some(AmmoMod::Turned));
        let every = AmmoMod::ALL.iter().fold(AmmoMods::NONE, |held, m| held.with(*m));
        assert_eq!(pick_ammo_mod(every, 0.3), None);
    }

    #[test]
    fn an_ammo_mod_triggers_on_a_roll_under_its_chance() {
        let cooldowns = Cooldowns::default();
        let dead_wire = held(&[AmmoMod::DeadWire]);
        assert_eq!(triggers(dead_wire, &cooldowns, ALIVE, rolls(&[0.19])), vec![AmmoMod::DeadWire]);
        assert!(triggers(dead_wire, &cooldowns, ALIVE, rolls(&[0.2])).is_empty());
        assert!(triggers(AmmoMods::NONE, &cooldowns, ALIVE, rolls(&[])).is_empty());
    }

    #[test]
    fn a_trigger_starts_a_cooldown_that_blocks_rolls_until_it_runs_out() {
        let mut cooldowns = Cooldowns::default();
        let thunder = held(&[AmmoMod::ThunderWall]);
        cooldowns.start(AmmoMod::ThunderWall, THUNDER_WALL_COOLDOWN_SECS);
        assert!(triggers(thunder, &cooldowns, ALIVE, rolls(&[])).is_empty(), "no roll while cooling down");
        cooldowns.tick(9.5);
        assert!(!cooldowns.ready(AmmoMod::ThunderWall));
        cooldowns.tick(0.5);
        assert!(cooldowns.ready(AmmoMod::ThunderWall));
        assert_eq!(triggers(thunder, &cooldowns, ALIVE, rolls(&[0.0])), vec![AmmoMod::ThunderWall]);
        // Other Ammo Mods keep their own cooldowns.
        assert!(cooldowns.ready(AmmoMod::DeadWire));
    }

    #[test]
    fn two_ammo_mods_roll_independently_and_can_both_trigger_on_one_hit() {
        let cooldowns = Cooldowns::default();
        let both = held(&[AmmoMod::DeadWire, AmmoMod::ThunderWall]);
        assert_eq!(
            triggers(both, &cooldowns, ALIVE, rolls(&[0.1, 0.1])),
            vec![AmmoMod::DeadWire, AmmoMod::ThunderWall]
        );
        assert_eq!(triggers(both, &cooldowns, ALIVE, rolls(&[0.5, 0.1])), vec![AmmoMod::ThunderWall]);
        assert_eq!(triggers(both, &cooldowns, ALIVE, rolls(&[0.1, 0.5])), vec![AmmoMod::DeadWire]);
    }

    #[test]
    fn turned_never_triggers_on_a_killing_hit() {
        let cooldowns = Cooldowns::default();
        let turned = held(&[AmmoMod::Turned]);
        let lethal = Struck { lethal: true, ..ALIVE };
        assert!(triggers(turned, &cooldowns, lethal, rolls(&[0.0])).is_empty());
        assert_eq!(triggers(turned, &cooldowns, ALIVE, rolls(&[0.0])), vec![AmmoMod::Turned]);
        // Another Ammo Mod's trigger kills the shot Zombie, leaving nothing to turn.
        let both = held(&[AmmoMod::BlastFurnace, AmmoMod::Turned]);
        assert_eq!(triggers(both, &cooldowns, ALIVE, rolls(&[0.0, 0.0])), vec![AmmoMod::BlastFurnace]);
        assert_eq!(triggers(both, &cooldowns, ALIVE, rolls(&[0.9, 0.0])), vec![AmmoMod::Turned]);
    }

    #[test]
    fn other_ammo_mods_trigger_on_a_killing_hit() {
        let cooldowns = Cooldowns::default();
        let lethal = Struck { lethal: true, ..ALIVE };
        assert_eq!(triggers(held(&[AmmoMod::Fireworks]), &cooldowns, lethal, rolls(&[0.0])), vec![AmmoMod::Fireworks]);
    }

    #[test]
    fn fireworks_and_turned_wait_for_a_zombie_to_climb_out() {
        let cooldowns = Cooldowns::default();
        let emerging = Struck { emerging: true, ..ALIVE };
        let all = AmmoMod::ALL.iter().fold(AmmoMods::NONE, |held, m| held.with(*m));
        let fired = triggers(all, &cooldowns, emerging, || 0.0);
        assert_eq!(fired, vec![AmmoMod::BlastFurnace, AmmoMod::DeadWire, AmmoMod::ThunderWall]);
    }

    #[test]
    fn dead_wire_jumps_shrink_until_the_chain_stops() {
        // A line of Zombies 2 m apart: the first jump (2.54 m) reaches, the second (2.03 m) just
        // does, the third (1.52 m) doesn't.
        let line: Vec<_> = (1..6).map(|i| Vec3::new(i as f32 * 2.0, 0.0, 0.0)).collect();
        let chain = dead_wire_chain(Vec3::ZERO, &line, |_, _| true);
        assert_eq!(
            chain,
            vec![
                Link { index: 0, from: None, depth: 1 },
                Link { index: 1, from: Some(0), depth: 2 },
            ]
        );
    }

    #[test]
    fn dead_wire_hits_at_most_9_and_needs_a_clear_line() {
        let crowd: Vec<_> = (0..20).map(|i| Vec3::new((i % 5) as f32 * 0.3, 0.0, (i / 5) as f32 * 0.3)).collect();
        assert_eq!(dead_wire_chain(Vec3::ZERO, &crowd, |_, _| true).len(), 8, "8 more and the shot one");
        assert!(dead_wire_chain(Vec3::ZERO, &crowd, |_, _| false).is_empty());
    }

    #[test]
    fn thunder_wall_kills_up_to_5_more_in_front_closest_first() {
        let mut others: Vec<_> = (1..=8).map(|i| Vec3::new(0.0, 0.0, -0.5 * i as f32)).collect();
        others.push(Vec3::new(0.0, 0.0, 1.0)); // behind the blast
        others.push(Vec3::new(0.0, 0.0, -10.0)); // out of reach
        let targets = thunder_wall_targets(Vec3::ZERO, Vec3::NEG_Z, &others);
        assert_eq!(targets, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn the_turn_kills_the_3_closest_within_2_3_m() {
        let others = [
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(0.5, 0.0, 0.0),
            Vec3::new(3.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.0, 0.0, -1.5),
        ];
        assert_eq!(turn_blast_targets(Vec3::ZERO, &others), vec![1, 3, 4]);
    }

    #[test]
    fn a_phantom_shot_checks_sight_for_only_3_candidates() {
        let candidates = [(0, 20.0), (1, 5.0), (2, 6.0), (3, 7.0), (4, 8.0)];
        assert_eq!(fireworks_target(&candidates, |i| i == 2), Some(2));
        assert_eq!(fireworks_target(&candidates, |_| false), Some(4), "the 4th in range, unchecked");
        assert_eq!(fireworks_target(&[(0, 20.0)], |_| true), None, "out of range");
    }

    #[test]
    fn a_blast_furnace_burn_kills_on_its_sixth_tick() {
        let burn = blast_furnace_burn(1200.0);
        let ticks = (burn.secs / burn.tick_secs).floor();
        assert_eq!(ticks, 6.0);
        assert!(burn.damage * 5.0 < 1200.0);
        assert!(burn.damage * 6.0 >= 1200.0);
    }

    #[test]
    fn the_glint_alternates_between_two_ammo_mods() {
        let purple = Color::srgb(0.6, 0.2, 1.0);
        assert_eq!(glint_colour(AmmoMods::NONE, 3, purple), purple);
        let one = held(&[AmmoMod::Fireworks]);
        assert_eq!(glint_colour(one, 0, purple), AmmoMod::Fireworks.glint());
        assert_eq!(glint_colour(one, 1, purple), AmmoMod::Fireworks.glint());
        let two = held(&[AmmoMod::Turned, AmmoMod::DeadWire]);
        assert_eq!(glint_colour(two, 0, purple), AmmoMod::DeadWire.glint());
        assert_eq!(glint_colour(two, 1, purple), AmmoMod::Turned.glint());
        assert_eq!(glint_colour(two, 2, purple), AmmoMod::DeadWire.glint());
    }

    #[test]
    fn only_ammo_mod_kills_choose_their_pay() {
        assert_eq!(Attack::AmmoMod(Pay::Unpaid).pay(), Pay::Unpaid);
        assert_eq!(Attack::AmmoMod(Pay::Paid).pay(), Pay::Paid);
        assert_eq!(Attack::Bullet.pay(), Pay::Paid);
    }
}
