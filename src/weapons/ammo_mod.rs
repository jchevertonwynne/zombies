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

use crate::weapons::arsenal::Burn;
use crate::audio::PlayCue;
use crate::net::fx::Fx;
use crate::audio::cues::Cue;
use crate::authority::HostSimulation;
use crate::weapons::burn::{Burning, Ignite};
use crate::maps::door::Door;
use crate::weapons::explosive::{in_sight, world_filter};
use crate::layers::GameLayer;
use crate::enemies::hellhound::Telegraph;
use crate::modifier::{Modifier, Modifiers, Stat, resolve, resolve_count};
use crate::maps::barrier::Barriers;
use crate::enemies::nav::{NavGraph, Route};
use crate::players::player::{Player, PlayerCamera};
use crate::state::{GameState, InRun, world_runs};
use crate::tuning::*;
use crate::enemies::zombie::{
    Attack, DamageZombie, Dying, Heavyweight, HitArea, HitZone, Lured, Pay, Rising, StunZombie, Zombie, ZombieKilled,
    apply_damage,
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

    /// The sound it makes when it triggers.
    pub fn cue(self) -> Cue {
        match self {
            AmmoMod::BlastFurnace => Cue::AmmoModBlastFurnace,
            AmmoMod::DeadWire => Cue::AmmoModDeadWire,
            AmmoMod::Fireworks => Cue::AmmoModFireworks,
            AmmoMod::ThunderWall => Cue::AmmoModThunderWall,
            AmmoMod::Turned => Cue::AmmoModTurned,
        }
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct AmmoMods(u8);

impl AmmoMods {
    pub const NONE: AmmoMods = AmmoMods(0);

    /// The Mods held, one bit each, for the wire.
    pub fn bits(self) -> u8 {
        self.0
    }

    /// The Mods a Player's game said they hold. A bit with no Ammo Mod behind it is ignored.
    pub fn from_bits(bits: u8) -> Self {
        Self(bits & ((1u16 << AmmoMod::ALL.len()) - 1) as u8)
    }

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

    /// Whether a tap Repack can add another, on a Weapon that holds `slots`.
    /// Every Ammo Mod in either.
    pub fn union(self, other: AmmoMods) -> AmmoMods {
        AmmoMods(self.0 | other.0)
    }

    pub fn has_room(self, slots: usize) -> bool {
        self.len() < slots
    }

    pub fn iter(self) -> impl Iterator<Item = AmmoMod> {
        AmmoMod::ALL.into_iter().filter(move |ammo_mod| self.contains(*ammo_mod))
    }
}

/// How many Ammo Mods an Upgraded Weapon holds: `MAX_AMMO_MODS`, or as many as the Player's stat
/// entries say.
pub fn ammo_mod_slots(player: &[Modifier]) -> usize {
    resolve_count(Stat::AmmoModSlots, MAX_AMMO_MODS as u32, player) as usize
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

/// Seconds left on each Ammo Mod's cooldown, per Player and shared by every Weapon of theirs
/// holding that Ammo Mod.
#[derive(Component, Debug, Default, Clone, PartialEq)]
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
#[allow(dead_code, reason = "the Ammo Mod rules without a Turned ally cap, as tests state them")]
pub fn triggers(held: AmmoMods, cooldowns: &Cooldowns, struck: Struck, roll: impl FnMut() -> f32) -> Vec<AmmoMod> {
    triggers_within(held, cooldowns, struck, true, roll)
}

/// `triggers`, with `turn_room` saying whether another Turned ally fits under the
/// `TurnedAllyCap`. Without room Turned isn't rolled at all, so it starts no cooldown.
pub fn triggers_within(
    held: AmmoMods,
    cooldowns: &Cooldowns,
    struck: Struck,
    turn_room: bool,
    mut roll: impl FnMut() -> f32,
) -> Vec<AmmoMod> {
    let mut fired = Vec::new();
    let mut lethal = struck.lethal;
    for ammo_mod in held.iter() {
        if !cooldowns.ready(ammo_mod)
            || (lethal && !ammo_mod.on_killing_hit())
            || (struck.emerging && !ammo_mod.on_emerging())
            || (ammo_mod == AmmoMod::Turned && !turn_room)
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
    chain(shot, others, DEAD_WIRE_MAX_ZOMBIES - 1, clear)
}

/// A chain out from `shot` at Dead Wire's jump radii, striking at most `max` of the others.
pub fn chain(shot: Vec3, others: &[Vec3], max: usize, clear: impl Fn(Vec3, Vec3) -> bool) -> Vec<Link> {
    let mut links: Vec<Link> = Vec::new();
    let mut hit = vec![false; others.len()];
    // Breadth first: (index or the shot, where it stands, its depth).
    let mut queue = std::collections::VecDeque::from([(None, shot, 0u32)]);
    while let Some((from, at, depth)) = queue.pop_front() {
        let Some(&radius) = DEAD_WIRE_JUMP_RADII.get(depth as usize) else { continue };
        for (index, &other) in others.iter().enumerate() {
            if links.len() >= max {
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

/// A fire Effect's `radius` after its Player's `Stat::FireRadius` entries.
pub fn fire_radius(radius: f32, player: &[Modifier]) -> f32 {
    resolve(Stat::FireRadius, 1.0, player) * radius
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
    /// The Player whose Weapon it was, whose Ammo Mods roll and whose kills they credit.
    pub player: Entity,
    pub zombie: Entity,
    pub damage: f32,
    pub ammo_mods: AmmoMods,
}

/// The most Turned allies alive at once, counting every source of them: at the cap a Turned
/// trigger doesn't fire and starts no cooldown. Classic has none (ADR 0007).
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TurnedAllyCap(pub Option<u32>);

impl TurnedAllyCap {
    /// Whether one more fits beside the `alive` allies.
    pub fn has_room(self, alive: usize) -> bool {
        self.0.is_none_or(|cap| alive < cap as usize)
    }
}

/// An Ammo Mod element a `CastEffect` asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Element {
    /// Dead Wire's shock.
    Shock,
    /// Blast Furnace's burn.
    Fire,
    /// Thunder Wall's fling, all round.
    Knockback,
    /// Fireworks' phantom gun.
    PhantomGun,
    /// A Turned ally.
    Turned,
}

/// Where a `CastEffect` lands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CastTarget {
    #[allow(dead_code, reason = "for Effects aimed at a Zombie, such as later Set Bonuses and Duos")]
    Zombie(Entity),
    Point(Vec3),
}

/// A request to cast an Ammo Mod element, as if a trigger had fired, but from a Zombie or a point
/// rather than a shot (ADR 0007). Its damage is `Attack::AmmoMod(pay)`, so it never rolls an Ammo
/// Mod. It plays no cue.
#[derive(Message, Debug, Clone, Copy)]
pub struct CastEffect {
    /// Whose it is, who its kills credit.
    pub player: Entity,
    pub element: Element,
    pub target: CastTarget,
    /// How big: Shock, the Zombies struck; Fire, its radius in metres; Knockback, the Zombies
    /// flung; PhantomGun, its shots; Turned, the ally's lifetime in seconds.
    pub size: f32,
    /// Shock: strike every Zombie this close to the target straight from it, rather than
    /// chaining. Knockback: how close a Zombie must be (Thunder Wall's radius without it).
    pub reach: Option<f32>,
    /// Fire: seconds it stays as a pool setting alight any Zombie that steps in; 0 burns once.
    pub lasts: f32,
    pub pay: Pay,
    /// A ring of this colour on the target for `CAST_ACCENT_SECS`.
    pub accent: Option<Color>,
    /// False skips every visual, but not the effect.
    pub show: bool,
    /// Turned: an ally with no lifetime or kill limit that sits outside the `TurnedAllyCap`
    /// (`size` is ignored).
    pub unbounded: bool,
    /// Turned: the ally glows this colour rather than Turned green.
    pub tint: Option<Color>,
    /// PhantomGun: each shot sets its Zombie alight (a Blast Furnace burn, which kills it on its
    /// last tick) rather than killing it outright.
    pub ignites: bool,
}

/// A Turned ally that doesn't count toward the `TurnedAllyCap`.
#[derive(Component, Debug, Default)]
pub struct OutsideAllyCap;

/// A Zombie on the Player's side after a Turned trigger. It's no longer a `Zombie`, so nothing
/// can hurt it and the Round doesn't wait for it.
#[derive(Component, Debug)]
pub struct TurnedAlly {
    /// The Player whose Weapon turned it, who its kills credit.
    player: Option<Entity>,
    /// Seconds it lives.
    pub(crate) lifetime: f32,
    /// The most Zombies it kills before it falls; `None` for no limit.
    pub(crate) kill_limit: Option<u32>,
    elapsed: f32,
    kills: u32,
    wind_up: Option<f32>,
    /// Seconds left of its last swing once its time or kills are up.
    ending: Option<f32>,
    fall_speed: f32,
}

impl Default for TurnedAlly {
    fn default() -> Self {
        Self {
            player: None,
            lifetime: TURNED_LIFETIME_SECS,
            kill_limit: Some(TURNED_MAX_KILLS),
            elapsed: 0.0,
            kills: 0,
            wind_up: None,
            ending: None,
            fall_speed: 0.0,
        }
    }
}

/// A Turned ally's time or kills ran out, and it fell where it stood.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct TurnedAllyExpired {
    pub at: Vec3,
    /// The Player whose Weapon or cast turned it.
    pub player: Option<Entity>,
}

impl TurnedAlly {
    /// The Player whose Weapon or cast turned it, who its kills credit.
    pub fn player(&self) -> Option<Entity> {
        self.player
    }

    /// An ally of `player`'s with the usual lifetime and kill limit, for a test to stand up.
    #[cfg(test)]
    pub fn of(player: Entity) -> Self {
        Self { player: Some(player), ..default() }
    }
}

/// A fire pool a `CastEffect` left: it sets alight every Zombie that stands in it.
#[derive(Component, Debug)]
struct FirePool {
    player: Entity,
    at: Vec3,
    radius: f32,
    left: f32,
    show: bool,
}

/// A body thrown by a Thunder Wall or a turn.
#[derive(Component, Debug)]
struct Flung {
    velocity: Vec3,
    /// How far above its takeoff height it has been thrown. It lands back on 0, wherever its fall
    /// has sunk it to meanwhile.
    height: f32,
}

/// The phantom gun Fireworks raises where the shot Zombie fell.
#[derive(Component, Debug)]
struct Phantom {
    /// The Player whose Weapon raised it.
    player: Entity,
    base: Vec3,
    elapsed: f32,
    shots: u32,
    /// The shots it fires in all.
    total: u32,
    pay: Pay,
    show: bool,
    /// Its shots set Zombies alight rather than killing them outright.
    ignites: bool,
    /// The Zombies an igniting phantom has set alight, which its later shots leave be.
    lit: Vec<Entity>,
}

/// Dead Wire hits waiting out their delay: (seconds left, Zombie, Player whose Weapon it was, its
/// pay).
#[derive(Resource, Default)]
struct DelayedHits(Vec<(f32, Entity, Entity, Pay)>);

/// Short-lived lines and bursts showing each Ammo Mod at work.
#[derive(Resource, Default)]
pub struct Flashes {
    flashes: Vec<Flash>,
    /// What the Host drew, for it to tell the Players (`crate::net::fx`).
    pub sent: Vec<Fx>,
}

struct Flash {
    shape: Shape,
    colour: Color,
    life: f32,
}

enum Shape {
    Line(Vec3, Vec3),
    Burst(Vec3, f32),
    /// A flat ring on the ground: (centre, radius).
    Ring(Vec3, f32),
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
            .add_message::<CastEffect>()
            .add_message::<TurnedAllyExpired>()
            .init_resource::<TurnedAllyCap>()
            .init_resource::<DelayedHits>()
            .init_resource::<Flashes>()
            .add_systems(Startup, load_assets)
            .add_systems(OnEnter(GameState::Playing), reset)
            .add_systems(
                Update,
                (
                    (roll_ammo_mods, cast_effects, burn_in_pools, run_delayed_hits, fire_phantoms)
                        .chain()
                        .in_set(HostSimulation)
                        .before(apply_damage),
                    (draw_flashes, draw_pools),
                )
                    .run_if(world_runs),
            )
            // Turned allies and flung bodies are Zombies' bodies, which are interpolated, so they
            // move in the physics step (see `crate::enemies::zombie::FrameTurnSystems`).
            .add_systems(FixedUpdate, (run_allies, fling).in_set(HostSimulation).run_if(world_runs));
    }
}

/// A glowing ally's or phantom's material in `colour`.
fn glow(colour: Color) -> StandardMaterial {
    StandardMaterial {
        base_color: colour,
        emissive: LinearRgba::from(colour) * 4.0,
        ..default()
    }
}

fn load_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(AmmoModAssets {
        ally: materials.add(glow(AmmoMod::Turned.glint())),
        phantom: materials.add(glow(AmmoMod::Fireworks.glint())),
        gun: meshes.add(Cuboid::new(0.08, 0.14, 0.6)),
    });
}

fn reset(mut delayed: ResMut<DelayedHits>, mut flashes: ResMut<Flashes>) {
    delayed.0.clear();
    flashes.flashes.clear();
    flashes.sent.clear();
}

/// Enough damage to kill any Zombie outright.
const KILL: f32 = f32::INFINITY;

fn kill(damage: &mut MessageWriter<DamageZombie>, player: Option<Entity>, zombie: Entity, pay: Pay) {
    damage.write(DamageZombie {
        player,
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
    mut delayed: ResMut<DelayedHits>,
    mut flashes: ResMut<Flashes>,
    // A hound still in its lightning strike can't be hurt, so no Ammo Mod touches it.
    zombies: Query<(Entity, &Zombie, &Transform, &Children, Has<Dying>, Has<Rising>, Has<Heavyweight>), Without<Telegraph>>,
    hit_areas: Query<(), With<HitArea>>,
    mut players: Query<(&Modifiers, &mut Cooldowns), With<Player>>,
    camera: Single<&GlobalTransform, With<PlayerCamera>>,
    spatial: SpatialQuery,
    assets: Res<AmmoModAssets>,
    mut damage: MessageWriter<DamageZombie>,
    mut ignite: MessageWriter<Ignite>,
    mut kills: MessageWriter<ZombieKilled>,
    (mut stuns, mut cues): (MessageWriter<StunZombie>, MessageWriter<PlayCue>),
    (cap, allies): (Res<TurnedAllyCap>, Query<(), (With<TurnedAlly>, Without<OutsideAllyCap>)>),
    time: Res<Time>,
) {
    let mut allies = allies.iter().count();
    for (_, mut cooldowns) in &mut players {
        cooldowns.tick(time.delta_secs());
    }
    // Zombies already killed or turned by a trigger this frame.
    let mut spent: Vec<Entity> = Vec::new();
    for hit in hits.read() {
        let Ok((modifiers, mut cooldowns)) = players.get_mut(hit.player) else {
            continue;
        };
        let Ok((shot, zombie, transform, children, dying, rising, heavyweight)) = zombies.get(hit.zombie) else {
            continue;
        };
        if spent.contains(&shot) {
            continue;
        }
        let struck = Struck {
            lethal: dying || zombie.health() <= 0.0 || hit.damage >= zombie.health(),
            emerging: rising,
        };
        let fired = triggers_within(hit.ammo_mods, &cooldowns, struck, cap.has_room(allies), rand::random::<f32>);
        if fired.is_empty() {
            continue;
        }
        let at = transform.translation;
        // Every other living Zombie, with where it stands.
        let (others, positions): (Vec<Entity>, Vec<Vec3>) = zombies
            .iter()
            .filter(|(entity, _, _, _, dying, ..)| *entity != shot && !dying && !spent.contains(entity))
            .map(|(entity, _, transform, ..)| (entity, transform.translation))
            .unzip();
        for ammo_mod in fired {
            // Turned does nothing to a Napalm, a Shrieker, the Sapper or any Heavyweight. Every
            // other proc only hurts a Heavyweight (see `heavyweight_damage`).
            if ammo_mod == AmmoMod::Turned && (zombie.special_kind().is_some() || heavyweight) {
                continue;
            }
            cooldowns.start(ammo_mod, resolve(Stat::AmmoModCooldown, ammo_mod.cooldown_secs(), &modifiers.0));
            cues.write(PlayCue::new(ammo_mod.cue(), Some(at)));
            let colour = ammo_mod.glint();
            match ammo_mod {
                AmmoMod::BlastFurnace => {
                    kill(&mut damage, Some(hit.player), shot, Pay::Paid);
                    let radius = fire_radius(BLAST_FURNACE_RADIUS, &modifiers.0);
                    for (i, &other) in positions.iter().enumerate() {
                        if at.distance(other) <= radius
                            && let Ok((_, target, ..)) = zombies.get(others[i])
                        {
                            ignite.write(Ignite {
                                player: hit.player,
                                zombie: others[i],
                                burn: blast_furnace_burn(target.health()),
                            });
                        }
                    }
                    flashes.burst(at, radius, colour);
                }
                AmmoMod::DeadWire => {
                    kill(&mut damage, Some(hit.player), shot, Pay::Paid);
                    if heavyweight {
                        stuns.write(StunZombie {
                            zombie: shot,
                            secs: SAPPER_DEAD_WIRE_STUN_SECS,
                        });
                    }
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
                        delayed.0.push((delay, zombie, hit.player, Pay::Paid));
                        spent.push(zombie);
                    }
                }
                AmmoMod::Fireworks => {
                    kill(&mut damage, Some(hit.player), shot, Pay::Paid);
                    let feet = at - Vec3::Y * (ZOMBIE_RADIUS + ZOMBIE_CAPSULE_LENGTH / 2.0);
                    commands.spawn((
                        InRun,
                        Name::new("Fireworks phantom"),
                        Phantom {
                            player: hit.player,
                            base: feet,
                            elapsed: 0.0,
                            shots: 0,
                            total: FIREWORKS_SHOTS,
                            pay: Pay::Unpaid,
                            show: true,
                            ignites: false,
                            lit: Vec::new(),
                        },
                        Mesh3d(assets.gun.clone()),
                        MeshMaterial3d(assets.phantom.clone()),
                        Transform::from_translation(feet),
                    ));
                    flashes.burst(at, 0.6, colour);
                }
                AmmoMod::ThunderWall => {
                    kill(&mut damage, Some(hit.player), shot, Pay::Paid);
                    let forward = camera.forward().with_y(0.0).normalize_or_zero();
                    let targets = thunder_wall_targets(at, forward, &positions);
                    // A Heavyweight survives it, so it stays on its feet.
                    if !heavyweight {
                        commands.entity(shot).insert(Flung::away(at - forward, at, THUNDER_WALL_FLING_SPEED, THUNDER_WALL_FLING_LIFT));
                    }
                    for i in targets {
                        kill(&mut damage, Some(hit.player), others[i], Pay::Paid);
                        if !zombies.get(others[i]).is_ok_and(|(.., heavyweight)| heavyweight) {
                            commands.entity(others[i]).insert(Flung::away(at, positions[i], THUNDER_WALL_FLING_SPEED, THUNDER_WALL_FLING_LIFT));
                        }
                        spent.push(others[i]);
                    }
                    flashes.burst(at, THUNDER_WALL_RADIUS, colour);
                }
                AmmoMod::Turned => {
                    allies += 1;
                    let special = |zombie| zombies.get(zombie).is_ok_and(|(_, other, .., heavyweight)| other.special_kind().is_some() || heavyweight);
                    let turn = Turn {
                        player: hit.player,
                        zombie: shot,
                        at,
                        lifetime: TURNED_LIFETIME_SECS,
                        show: true,
                        unbounded: false,
                        material: assets.ally.clone(),
                    };
                    spent.extend(turn.apply(
                        &mut commands,
                        children,
                        &hit_areas,
                        (&others, &positions, special),
                        (&mut kills, &mut damage, &mut flashes),
                    ));
                }
            }
        }
        spent.push(shot);
    }
}

/// One Zombie turned: it counts toward the Round as an Unpaid Kill and lives on as an ally for
/// `lifetime`, and its turn kills (Unpaid) and throws the closest Zombies around it, leaving
/// Napalms and Shriekers be.
struct Turn {
    player: Entity,
    zombie: Entity,
    at: Vec3,
    lifetime: f32,
    show: bool,
    /// No lifetime or kill limit, and outside the `TurnedAllyCap`.
    unbounded: bool,
    material: Handle<StandardMaterial>,
}

impl Turn {
    /// Plays the turn out among the `others` at `positions`, returning the Zombies its blast
    /// killed.
    fn apply(
        self,
        commands: &mut Commands,
        children: &Children,
        hit_areas: &Query<(), With<HitArea>>,
        (others, positions, special): (&[Entity], &[Vec3], impl Fn(Entity) -> bool),
        (kills, damage, flashes): (&mut MessageWriter<ZombieKilled>, &mut MessageWriter<DamageZombie>, &mut Flashes),
    ) -> Vec<Entity> {
        let Turn { player, zombie, at, lifetime, show, unbounded, material } = self;
        kills.write(ZombieKilled {
            player: Some(player),
            pay: Pay::Unpaid,
            headshot: false,
            zone: HitZone::Torso,
            attack: Attack::AmmoMod(Pay::Unpaid),
            at,
            surge: false,
            zombie,
        });
        let mut ally = commands.entity(zombie);
        ally.remove::<(Zombie, Lured, Burning)>().insert((
            TurnedAlly {
                player: Some(player),
                lifetime: if unbounded { f32::INFINITY } else { lifetime },
                kill_limit: if unbounded { None } else { Some(TURNED_MAX_KILLS) },
                ..default()
            },
            MeshMaterial3d(material),
        ));
        if unbounded {
            ally.insert(OutsideAllyCap);
        }
        // Bullets pass through it.
        for child in children.iter() {
            if hit_areas.contains(child) {
                commands.entity(child).remove::<(Collider, CollisionLayers)>();
            }
        }
        let mut blasted = Vec::new();
        for i in turn_blast_targets(at, positions) {
            if special(others[i]) {
                continue;
            }
            kill(damage, Some(player), others[i], Pay::Unpaid);
            commands.entity(others[i]).insert(Flung::away(at, positions[i], TURNED_FLING_SPEED, TURNED_FLING_LIFT));
            blasted.push(others[i]);
        }
        if show {
            flashes.burst(at, TURNED_BLAST_RADIUS, AmmoMod::Turned.glint());
        }
        blasted
    }
}

/// Each `CastEffect` plays out as its Ammo Mod's trigger would, from its target: a Zombie, or a
/// point. A Turned cast at the `TurnedAllyCap` turns nothing.
fn cast_effects(
    mut commands: Commands,
    mut casts: MessageReader<CastEffect>,
    mut delayed: ResMut<DelayedHits>,
    mut flashes: ResMut<Flashes>,
    zombies: Query<(Entity, &Zombie, &Transform, &Children, Has<Dying>, Has<Rising>, Has<Heavyweight>), Without<Telegraph>>,
    hit_areas: Query<(), With<HitArea>>,
    spatial: SpatialQuery,
    assets: Res<AmmoModAssets>,
    (cap, allies): (Res<TurnedAllyCap>, Query<(), (With<TurnedAlly>, Without<OutsideAllyCap>)>),
    players: Query<&Modifiers, With<Player>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut damage: MessageWriter<DamageZombie>,
    mut ignite: MessageWriter<Ignite>,
    mut kills: MessageWriter<ZombieKilled>,
) {
    let mut allies = allies.iter().count();
    // Zombies already killed or turned by a cast this frame.
    let mut spent: Vec<Entity> = Vec::new();
    for cast in casts.read() {
        let target = match cast.target {
            CastTarget::Zombie(zombie) => zombies
                .get(zombie)
                .ok()
                .filter(|(_, _, _, _, dying, ..)| !dying && !spent.contains(&zombie))
                .map(|(entity, _, transform, ..)| (Some(entity), transform.translation)),
            CastTarget::Point(at) => Some((None, at)),
        };
        let Some((struck, at)) = target else {
            continue;
        };
        // Every other living Zombie, with where it stands.
        let (others, positions): (Vec<Entity>, Vec<Vec3>) = zombies
            .iter()
            .filter(|(entity, _, _, _, dying, ..)| Some(*entity) != struck && !dying && !spent.contains(entity))
            .map(|(entity, _, transform, ..)| (entity, transform.translation))
            .unzip();
        let player = cast.player;
        let show = cast.show;
        match cast.element {
            Element::Shock => {
                let colour = AmmoMod::DeadWire.glint();
                let count = cast.size.max(0.0) as usize;
                let links: Vec<Link> = match cast.reach {
                    Some(reach) => closest_within(at, &positions, reach, count, |_| true)
                        .into_iter()
                        .map(|index| Link { index, from: None, depth: 1 })
                        .collect(),
                    None => chain(at, &positions, count, |a, b| in_sight(&spatial, a + Vec3::Y * 0.6, b + Vec3::Y * 0.6)),
                };
                if let Some(zombie) = struck {
                    kill(&mut damage, Some(player), zombie, cast.pay);
                    spent.push(zombie);
                }
                for link in links {
                    if show {
                        let from = link.from.map_or(at, |from| positions[from]);
                        flashes.line(from, positions[link.index], colour);
                    }
                    let delay = if link.depth > 1 {
                        rand::random_range(DEAD_WIRE_DELAY_MIN_SECS..DEAD_WIRE_DELAY_MAX_SECS) * link.depth as f32
                    } else {
                        0.0
                    };
                    delayed.0.push((delay, others[link.index], player, cast.pay));
                    spent.push(others[link.index]);
                }
                if show {
                    flashes.burst(at, 0.6, colour);
                }
            }
            Element::Fire => {
                let entries = players.get(player).map_or(&[][..], |modifiers| &modifiers.0[..]);
                let radius = fire_radius(cast.size.max(0.0), entries);
                for (i, &other) in positions.iter().enumerate() {
                    if at.with_y(0.0).distance(other.with_y(0.0)) <= radius
                        && let Ok((_, target, ..)) = zombies.get(others[i])
                    {
                        ignite.write(Ignite {
                            player,
                            zombie: others[i],
                            burn: blast_furnace_burn(target.health()),
                        });
                    }
                }
                if cast.lasts > 0.0 {
                    commands.spawn((
                        InRun,
                        Name::new("Fire pool"),
                        FirePool {
                            player,
                            at,
                            radius,
                            left: cast.lasts,
                            show,
                        },
                    ));
                }
                if show {
                    flashes.burst(at, radius, AmmoMod::BlastFurnace.glint());
                }
            }
            Element::Knockback => {
                let reach = cast.reach.unwrap_or(THUNDER_WALL_RADIUS);
                let count = cast.size.max(0.0) as usize;
                if let Some(zombie) = struck {
                    kill(&mut damage, Some(player), zombie, cast.pay);
                    spent.push(zombie);
                }
                for i in closest_within(at, &positions, reach, count, |_| true) {
                    kill(&mut damage, Some(player), others[i], cast.pay);
                    // The Sapper survives it, so it stays on its feet.
                    if !zombies.get(others[i]).is_ok_and(|(.., heavyweight)| heavyweight) {
                        commands
                            .entity(others[i])
                            .insert(Flung::away(at, positions[i], THUNDER_WALL_FLING_SPEED, THUNDER_WALL_FLING_LIFT));
                    }
                    spent.push(others[i]);
                }
                if show {
                    flashes.burst(at, reach, AmmoMod::ThunderWall.glint());
                }
            }
            Element::PhantomGun => {
                let feet = match struck {
                    Some(_) => at - Vec3::Y * (ZOMBIE_RADIUS + ZOMBIE_CAPSULE_LENGTH / 2.0),
                    None => at,
                };
                let mut phantom = commands.spawn((
                    InRun,
                    Name::new("Phantom gun"),
                    Phantom {
                        player,
                        base: feet,
                        elapsed: 0.0,
                        shots: 0,
                        total: cast.size.max(0.0) as u32,
                        pay: cast.pay,
                        show,
                        ignites: cast.ignites,
                        lit: Vec::new(),
                    },
                    Transform::from_translation(feet),
                ));
                if show {
                    phantom.insert((Mesh3d(assets.gun.clone()), MeshMaterial3d(assets.phantom.clone())));
                    flashes.burst(at, 0.6, AmmoMod::Fireworks.glint());
                }
            }
            Element::Turned => {
                if !cast.unbounded && !cap.has_room(allies) {
                    continue;
                }
                // The Zombie asked for, or the nearest one to the point that's out of the ground.
                let turned = match struck {
                    Some(zombie) => zombies.get(zombie).ok(),
                    None => zombies
                        .iter()
                        .filter(|(entity, _, _, _, dying, rising, _)| !dying && !rising && !spent.contains(entity))
                        .filter(|(_, zombie, .., heavyweight)| zombie.special_kind().is_none() && !heavyweight)
                        .min_by(|a, b| a.2.translation.distance(at).total_cmp(&b.2.translation.distance(at))),
                };
                let Some((zombie, body, transform, children, _, rising, heavyweight)) = turned else {
                    continue;
                };
                // Turned does nothing to a Napalm, a Shrieker, the Sapper or any Heavyweight, nor
                // to a Zombie still climbing out.
                if body.special_kind().is_some() || heavyweight || rising {
                    continue;
                }
                if !cast.unbounded {
                    allies += 1;
                }
                let at = transform.translation;
                let (others, positions): (Vec<Entity>, Vec<Vec3>) = zombies
                    .iter()
                    .filter(|(entity, _, _, _, dying, ..)| *entity != zombie && !dying && !spent.contains(entity))
                    .map(|(entity, _, transform, ..)| (entity, transform.translation))
                    .unzip();
                let special = |zombie| zombies.get(zombie).is_ok_and(|(_, other, .., heavyweight)| other.special_kind().is_some() || heavyweight);
                let turn = Turn {
                    player,
                    zombie,
                    at,
                    lifetime: cast.size,
                    show,
                    unbounded: cast.unbounded,
                    material: cast.tint.map_or_else(|| assets.ally.clone(), |tint| materials.add(glow(tint))),
                };
                spent.push(zombie);
                spent.extend(turn.apply(
                    &mut commands,
                    children,
                    &hit_areas,
                    (&others, &positions, special),
                    (&mut kills, &mut damage, &mut flashes),
                ));
            }
        }
        if let Some(colour) = cast.accent {
            flashes.ring(at, CAST_ACCENT_RADIUS, colour, CAST_ACCENT_SECS);
        }
    }
}

/// Fire pools set alight every Zombie standing in them that isn't burning already, until they go
/// out.
fn burn_in_pools(
    mut commands: Commands,
    mut pools: Query<(Entity, &mut FirePool)>,
    zombies: Query<(Entity, &Zombie, &Transform), (Without<Dying>, Without<Burning>, Without<Telegraph>)>,
    mut ignite: MessageWriter<Ignite>,
    time: Res<Time>,
) {
    for (entity, mut pool) in &mut pools {
        pool.left -= time.delta_secs();
        if pool.left <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        for (zombie, body, transform) in &zombies {
            if transform.translation.with_y(0.0).distance(pool.at.with_y(0.0)) <= pool.radius {
                ignite.write(Ignite {
                    player: pool.player,
                    zombie,
                    burn: blast_furnace_burn(body.health()),
                });
            }
        }
    }
}

fn draw_pools(pools: Query<&FirePool>, mut gizmos: Gizmos) {
    let flat = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    for pool in pools.iter().filter(|pool| pool.show) {
        gizmos.circle(Isometry3d::new(pool.at + Vec3::Y * 0.05, flat), pool.radius, AmmoMod::BlastFurnace.glint());
    }
}

/// Dead Wire hits land once their delay is up, each dealing the Zombie's health and then some.
/// The Sapper is stunned instead of killed.
fn run_delayed_hits(
    mut delayed: ResMut<DelayedHits>,
    zombies: Query<(&Zombie, Has<Heavyweight>), Without<Dying>>,
    mut damage: MessageWriter<DamageZombie>,
    mut stuns: MessageWriter<StunZombie>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    delayed.0.retain_mut(|(left, zombie, player, pay)| {
        *left -= dt;
        if *left > 0.0 {
            return true;
        }
        if let Ok((target, heavyweight)) = zombies.get(*zombie) {
            if heavyweight {
                stuns.write(StunZombie {
                    zombie: *zombie,
                    secs: SAPPER_DEAD_WIRE_STUN_SECS,
                });
            }
            damage.write(DamageZombie {
                player: Some(*player),
                zombie: *zombie,
                damage: target.health() + DEAD_WIRE_EXTRA_DAMAGE,
                zone: HitZone::Torso,
                attack: Attack::AmmoMod(*pay),
            });
        }
        false
    });
}

/// The phantom gun rises, fires its shots at random Zombies, each a kill (Unpaid), then sinks away.
fn fire_phantoms(
    mut commands: Commands,
    mut phantoms: Query<(Entity, &mut Phantom, &mut Transform)>,
    zombies: Query<(Entity, &GlobalTransform, &Zombie, Has<Burning>), Without<Dying>>,
    mut flashes: ResMut<Flashes>,
    spatial: SpatialQuery,
    mut damage: MessageWriter<DamageZombie>,
    mut ignite: MessageWriter<Ignite>,
    time: Res<Time>,
) {
    let mut doomed: Vec<Entity> = Vec::new();
    for (entity, mut phantom, mut transform) in &mut phantoms {
        phantom.elapsed += time.delta_secs();
        let t = phantom.elapsed;
        let firing_ends = FIREWORKS_RISE_SECS + phantom.total as f32 * FIREWORKS_SHOT_SECS;
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

        while phantom.shots < phantom.total
            && t >= FIREWORKS_RISE_SECS + phantom.shots as f32 * FIREWORKS_SHOT_SECS
        {
            phantom.shots += 1;
            // An igniting phantom leaves Zombies already alight for its other shots.
            let mut candidates: Vec<_> = zombies
                .iter()
                .filter(|(zombie, _, _, burning)| {
                    !doomed.contains(zombie) && !(phantom.ignites && (*burning || phantom.lit.contains(zombie)))
                })
                .map(|(zombie, at, ..)| (zombie, at.translation()))
                .collect();
            candidates.sort_by_cached_key(|_| rand::random::<u32>());
            let ranked: Vec<_> = candidates.iter().enumerate().map(|(i, (_, at))| (i, muzzle.distance(*at))).collect();
            let target = fireworks_target(&ranked, |i| in_sight(&spatial, muzzle, candidates[i].1));
            let aim = match target {
                Some(i) => {
                    let (zombie, at) = candidates[i];
                    match zombies.get(zombie) {
                        Ok((_, _, body, _)) if phantom.ignites => {
                            phantom.lit.push(zombie);
                            ignite.write(Ignite {
                                player: phantom.player,
                                zombie,
                                burn: blast_furnace_burn(body.health()),
                            });
                        }
                        _ => kill(&mut damage, Some(phantom.player), zombie, phantom.pay),
                    }
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
            if phantom.show {
                flashes.line(muzzle, aim, AmmoMod::Fireworks.glint());
            }
        }
    }
}

/// Turned allies sprint at the closest Zombie and kill it with one swipe (Unpaid), until their
/// time or kills run out; then they fall, with no Points for anyone.
fn run_allies(
    mut commands: Commands,
    mut allies: Query<(Entity, &mut TurnedAlly, &mut Route, &mut Transform, &Collider), Without<Zombie>>,
    zombies: Query<(Entity, &Transform, &Zombie), (Without<Dying>, Without<Rising>)>,
    graph: Res<NavGraph>,
    barriers: Res<Barriers>,
    doors: Query<&Door>,
    pieces: Query<&crate::maps::shifting::PieceState>,
    move_and_slide: MoveAndSlide,
    mut damage: MessageWriter<DamageZombie>,
    mut expired: MessageWriter<TurnedAllyExpired>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    let half = ZOMBIE_RADIUS + ZOMBIE_CAPSULE_LENGTH / 2.0;
    let open_doors: Vec<_> = doors.iter().filter(|door| door.open).filter_map(|door| door.route()).collect();
    let rests = crate::maps::shifting::standing_rests(pieces.iter().map(|state| (state.id(), state.phase())), graph.map.layout().shifting.len());
    let barrier_open = |b| barriers.is_open(b);
    let nav = crate::enemies::nav::NavState::new(&open_doors, &barrier_open).with_rests(&rests);
    let filter_mask = [GameLayer::World, GameLayer::Zombie, GameLayer::Player];
    for (entity, mut ally, mut route, mut transform, collider) in &mut allies {
        ally.elapsed += dt;
        let killed_enough = ally.kill_limit.is_some_and(|limit| ally.kills >= limit);
        if ally.ending.is_none() && (ally.elapsed >= ally.lifetime || killed_enough) {
            ally.ending = Some(TURNED_LAST_SWING_SECS);
        }
        if let Some(left) = &mut ally.ending {
            *left -= dt;
            if *left <= 0.0 {
                expired.write(TurnedAllyExpired {
                    at: transform.translation,
                    player: ally.player,
                });
                commands
                    .entity(entity)
                    .remove::<(TurnedAlly, Collider, CollisionLayers)>()
                    .insert(Dying::default());
                continue;
            }
        }

        let feet = transform.translation - Vec3::Y * half;
        // An ally leaves a Napalm or a Shrieker alone.
        let target = zombies
            .iter()
            .filter(|(_, _, zombie)| zombie.special_kind().is_none())
            .map(|(zombie, at, _)| (zombie, at))
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
                    if in_reach && ally.kill_limit.is_none_or(|limit| ally.kills < limit) {
                        kill(&mut damage, ally.player, zombie, Pay::Unpaid);
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
                route.waypoints = crate::enemies::zombie::plan_route(
                    &graph,
                    &move_and_slide.spatial_query,
                    transform.translation,
                    half,
                    at - Vec3::Y * half,
                    ZOMBIE_REACH,
                    nav,
                );
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
            height: 0.0,
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
        // Down no further than back to its takeoff height, so it lands where its fall leaves it.
        let rise = (flung.velocity.y * dt).max(-flung.height);
        transform.translation.y += rise;
        flung.height += rise;
        flung.velocity.y -= GRAVITY * dt;
        if flung.velocity.y < 0.0 && flung.height <= 0.0 {
            commands.entity(entity).remove::<Flung>();
        }
    }
}

impl Flashes {
    pub fn line(&mut self, from: Vec3, to: Vec3, colour: Color) {
        self.sent.push(Fx::AmmoLine { from, to, colour });
        self.hear(Fx::AmmoLine { from, to, colour });
    }

    pub fn burst(&mut self, at: Vec3, radius: f32, colour: Color) {
        self.sent.push(Fx::AmmoBurst { at, radius, colour });
        self.hear(Fx::AmmoBurst { at, radius, colour });
    }

    pub fn ring(&mut self, at: Vec3, radius: f32, colour: Color, secs: f32) {
        self.sent.push(Fx::AmmoRing { at, radius, colour, secs });
        self.hear(Fx::AmmoRing { at, radius, colour, secs });
    }

    /// Shows an effect the Host told of, which is not told on.
    pub fn hear(&mut self, fx: Fx) {
        let (shape, colour, life) = match fx {
            Fx::AmmoLine { from, to, colour } => (Shape::Line(from, to), colour, FLASH_SECS),
            Fx::AmmoBurst { at, radius, colour } => (Shape::Burst(at, radius), colour, FLASH_SECS),
            Fx::AmmoRing { at, radius, colour, secs } => (Shape::Ring(at, radius), colour, secs),
            _ => return,
        };
        self.flashes.push(Flash { shape, colour, life });
    }

    /// How many are showing.
    #[cfg(test)]
    pub fn showing(&self) -> usize {
        self.flashes.len()
    }
}

fn draw_flashes(mut flashes: ResMut<Flashes>, mut gizmos: Gizmos, time: Res<Time>) {
    let dt = time.delta_secs();
    flashes.flashes.retain_mut(|flash| {
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
            Shape::Ring(at, radius) => {
                let flat = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
                gizmos.circle(Isometry3d::new(at, flat), radius, flash.colour);
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
        let slots = ammo_mod_slots(&[]);
        assert_eq!(slots, 2);
        assert!(AmmoMods::NONE.has_room(slots));
        assert!(held(&[AmmoMod::Turned]).has_room(slots));
        let full = held(&[AmmoMod::Turned, AmmoMod::DeadWire]);
        assert_eq!(full.len(), 2);
        assert!(!full.has_room(slots));
    }

    #[test]
    fn a_slot_entry_makes_room_for_a_third_ammo_mod() {
        let one = [Modifier {
            stat: Stat::AmmoModSlots,
            source: crate::modifier::Source("test"),
            effect: crate::modifier::Effect::Flat(1.0),
        }];
        let slots = ammo_mod_slots(&one);
        assert_eq!(slots, 3);
        assert!(held(&[AmmoMod::Turned, AmmoMod::DeadWire]).has_room(slots));
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
    fn turned_never_triggers_at_the_ally_cap_so_starts_no_cooldown() {
        let cooldowns = Cooldowns::default();
        let turned = held(&[AmmoMod::Turned]);
        assert!(triggers_within(turned, &cooldowns, ALIVE, false, rolls(&[])).is_empty(), "not even rolled");
        assert_eq!(triggers_within(turned, &cooldowns, ALIVE, true, rolls(&[0.0])), vec![AmmoMod::Turned]);
        // The other Ammo Mods roll as ever at the cap.
        let both = held(&[AmmoMod::DeadWire, AmmoMod::Turned]);
        assert_eq!(triggers_within(both, &cooldowns, ALIVE, false, rolls(&[0.0])), vec![AmmoMod::DeadWire]);
    }

    #[test]
    fn the_turned_ally_cap_is_neutral_until_set() {
        assert_eq!(TurnedAllyCap::default(), TurnedAllyCap(None));
        assert!(TurnedAllyCap(None).has_room(1000));
        assert!(TurnedAllyCap(Some(3)).has_room(2));
        assert!(!TurnedAllyCap(Some(3)).has_room(3));
    }

    #[test]
    fn a_cast_shock_chain_is_capped_at_its_size() {
        let crowd: Vec<_> = (0..20).map(|i| Vec3::new((i % 5) as f32 * 0.3, 0.0, (i / 5) as f32 * 0.3)).collect();
        assert_eq!(chain(Vec3::ZERO, &crowd, 3, |_, _| true).len(), 3);
        assert_eq!(chain(Vec3::ZERO, &crowd, 7, |_, _| true).len(), 7);
    }

    #[test]
    fn only_ammo_mod_kills_choose_their_pay() {
        assert_eq!(Attack::AmmoMod(Pay::Unpaid).pay(), Pay::Unpaid);
        assert_eq!(Attack::AmmoMod(Pay::Paid).pay(), Pay::Paid);
        assert_eq!(Attack::Bullet.pay(), Pay::Paid);
    }

    mod headless {
        use super::*;
        use bevy::ecs::system::RunSystemOnce;
        use crate::enemies::zombie::headless::Sim;

        const FRAME_RATES: [f64; 3] = [144.0, 60.0, 90.0];

        fn movement(app: &mut App) {
            app.add_message::<TurnedAllyExpired>().add_systems(FixedUpdate, (run_allies, fling));
        }

        /// Every kill the Sim has seen.
        #[derive(Resource, Default)]
        struct Kills(Vec<ZombieKilled>);

        /// The `CastEffect` seam on its own, with the Zombie systems.
        fn casting(cap: Option<u32>) -> impl FnOnce(&mut App) {
            move |app: &mut App| {
                app.add_message::<AmmoModHit>()
                    .add_message::<CastEffect>()
                    .add_message::<Ignite>()
                    .add_message::<PlayCue>()
                    .add_message::<TurnedAllyExpired>()
                    .init_resource::<DelayedHits>()
                    .init_resource::<Flashes>()
                    .init_resource::<Kills>()
                    .insert_resource(TurnedAllyCap(cap))
                    .add_systems(Update, (cast_effects, run_delayed_hits, fire_phantoms).chain().before(apply_damage))
                    .add_systems(PostUpdate, |mut read: MessageReader<ZombieKilled>, mut kills: ResMut<Kills>| {
                        kills.0.extend(read.read().copied());
                    })
                    .add_systems(FixedUpdate, (run_allies, fling));
                app.world_mut().run_system_once(load_assets).unwrap();
            }
        }

        fn player(sim: &mut Sim) -> Entity {
            let world = sim.world_mut();
            world.query_filtered::<Entity, With<Player>>().single(world).unwrap()
        }

        fn cast(player: Entity, element: Element, target: CastTarget, size: f32, pay: Pay) -> CastEffect {
            CastEffect { player, element, target, size, reach: None, lasts: 0.0, pay, accent: None, show: true, unbounded: false, tint: None, ignites: false }
        }

        #[test]
        fn cast_effect_damage_is_ammo_mod_damage_at_its_pay_and_never_rolls_an_ammo_mod() {
            for pay in [Pay::Paid, Pay::Unpaid] {
                let mut sim = Sim::new(60.0, casting(None));
                let feet = Vec3::new(4.0, 0.0, 12.0);
                sim.spawn(feet, None);
                sim.spawn(feet + Vec3::X, None);
                sim.run(5);
                let player = player(&mut sim);
                sim.send(cast(player, Element::Knockback, CastTarget::Point(feet), 6.0, pay));
                sim.run(3);
                let kills = &sim.world_mut().resource::<Kills>().0;
                assert_eq!(kills.len(), 2, "{pay:?}");
                assert!(kills.iter().all(|kill| kill.attack == Attack::AmmoMod(pay) && kill.pay == pay));
                assert!(kills.iter().all(|kill| kill.player == Some(player)));
                assert!(sim.world_mut().resource::<Messages<AmmoModHit>>().is_empty(), "no Ammo Mod roll");
            }
        }

        #[test]
        fn a_turned_ally_whose_time_runs_out_says_where_it_fell_and_whose_it_was() {
            let mut sim = Sim::new(60.0, casting(None));
            let feet = Vec3::new(4.0, 0.0, 12.0);
            let zombie = sim.spawn(feet, None);
            sim.run(5);
            let player = player(&mut sim);
            sim.send(cast(player, Element::Turned, CastTarget::Zombie(zombie), 0.5, Pay::Unpaid));
            let mut expired = Vec::new();
            for _ in 0..sim.frames(0.5 + TURNED_LAST_SWING_SECS + 0.5) {
                sim.run(1);
                expired.extend(sim.world_mut().resource_mut::<Messages<TurnedAllyExpired>>().drain());
            }
            assert_eq!(expired.len(), 1, "it expires once");
            assert_eq!(expired[0].player, Some(player));
            assert!(expired[0].at.with_y(0.0).distance(feet.with_y(0.0)) < 2.0, "{} is far from {feet}", expired[0].at);
        }

        #[test]
        fn an_igniting_phantom_gun_sets_its_zombies_alight_rather_than_killing_them() {
            let mut sim = Sim::new(60.0, casting(None));
            let feet = Vec3::new(4.0, 0.0, 12.0);
            let zombies = [sim.spawn(feet + Vec3::X * 2.0, None), sim.spawn(feet - Vec3::X * 2.0, None)];
            sim.run(5);
            let player = player(&mut sim);
            sim.send(CastEffect { ignites: true, ..cast(player, Element::PhantomGun, CastTarget::Point(feet), 2.0, Pay::Unpaid) });
            let mut ignited = Vec::new();
            for _ in 0..sim.frames(FIREWORKS_RISE_SECS + 3.0 * FIREWORKS_SHOT_SECS) {
                sim.run(1);
                ignited.extend(sim.world_mut().resource_mut::<Messages<Ignite>>().drain().map(|ignite| (ignite.player, ignite.zombie)));
            }
            ignited.sort();
            let mut expected = zombies.map(|zombie| (player, zombie)).to_vec();
            expected.sort();
            assert_eq!(ignited, expected, "each shot sets one Zombie alight, credited to the cast's Player");
            assert!(sim.world_mut().resource::<Kills>().0.is_empty(), "no shot kills outright");
        }

        #[test]
        fn a_cast_turn_at_the_ally_cap_turns_nothing() {
            for (cap, turns) in [(None, true), (Some(1), true), (Some(0), false)] {
                let mut sim = Sim::new(60.0, casting(cap));
                let zombie = sim.spawn(Vec3::new(4.0, 0.0, 12.0), None);
                sim.run(5);
                let player = player(&mut sim);
                sim.send(cast(player, Element::Turned, CastTarget::Zombie(zombie), 7.0, Pay::Unpaid));
                sim.run(3);
                let world = sim.world_mut();
                assert_eq!(world.get::<TurnedAlly>(zombie).is_some(), turns, "cap {cap:?}");
                assert_eq!(world.get::<Zombie>(zombie).is_none(), turns, "cap {cap:?}");
                if turns {
                    assert_eq!(world.get::<TurnedAlly>(zombie).unwrap().lifetime, 7.0, "lives as long as asked");
                }
            }
        }

        #[test]
        fn an_unbounded_cast_turn_ignores_the_cap_and_never_fills_it() {
            let mut sim = Sim::new(60.0, casting(Some(1)));
            let guard = sim.spawn(Vec3::new(4.0, 0.0, 12.0), None);
            let other = sim.spawn(Vec3::new(-4.0, 0.0, 12.0), None);
            sim.run(5);
            let player = player(&mut sim);
            let tint = Color::srgb(1.0, 0.0, 0.0);
            sim.send(CastEffect { unbounded: true, tint: Some(tint), ..cast(player, Element::Turned, CastTarget::Zombie(guard), 0.0, Pay::Unpaid) });
            sim.run(3);
            let world = sim.world_mut();
            let ally = world.get::<TurnedAlly>(guard).expect("turned past the cap");
            assert!(ally.lifetime.is_infinite() && ally.kill_limit.is_none(), "no lifetime or kill limit");
            assert!(world.get::<OutsideAllyCap>(guard).is_some());
            // It doesn't count toward the cap of 1, so an ordinary turn still has room.
            sim.send(cast(player, Element::Turned, CastTarget::Zombie(other), 7.0, Pay::Unpaid));
            sim.run(3);
            assert!(sim.world_mut().get::<TurnedAlly>(other).is_some());
        }

        #[test]
        fn a_cast_phantom_gun_fires_its_size_in_shots() {
            let mut sim = Sim::new(60.0, casting(None));
            let feet = Vec3::new(4.0, 0.0, 12.0);
            for i in 0..6 {
                sim.spawn(feet + Vec3::new(i as f32 * 0.8 - 2.0, 0.0, 3.0), None);
            }
            sim.run(5);
            let player = player(&mut sim);
            sim.send(cast(player, Element::PhantomGun, CastTarget::Point(feet), 2.0, Pay::Unpaid));
            sim.run(sim.frames(FIREWORKS_RISE_SECS + 10.0 * FIREWORKS_SHOT_SECS + 1.0));
            let kills = &sim.world_mut().resource::<Kills>().0;
            assert_eq!(kills.len(), 2);
            assert!(kills.iter().all(|kill| kill.attack == Attack::AmmoMod(Pay::Unpaid)));
        }

        /// Throwing a body from `Update` was undone whenever the next frame ran a physics step, so
        /// flung bodies barely moved, and landing put a body back at standing height however far
        /// its fall had sunk it. A Thunder Wall throws one its whole arc, evenly, at any frame
        /// rate, and it lands lying on the floor.
        #[test]
        fn a_flung_body_flies_its_whole_arc_and_lands_lying_down() {
            let flight = 2.0 * THUNDER_WALL_FLING_LIFT / GRAVITY;
            let reach = THUNDER_WALL_FLING_SPEED * flight;
            for fps in FRAME_RATES {
                let mut sim = Sim::new(fps, movement);
                let zombie = sim.spawn(Vec3::new(4.0, 0.0, 12.0), None);
                sim.run(sim.frames(0.3));
                let from = sim.frame(zombie).pos;
                sim.send(DamageZombie {
                    player: None,
                    zombie,
                    damage: 1e6,
                    zone: HitZone::Torso,
                    attack: Attack::AmmoMod(Pay::Paid),
                });
                let blast = from + Vec3::X;
                sim.world_mut().entity_mut(zombie).insert(Flung::away(blast, from, THUNDER_WALL_FLING_SPEED, THUNDER_WALL_FLING_LIFT));
                let samples = sim.track(zombie, sim.frames(flight + 0.5));
                // At its speed every frame in the air, give or take the steps at either end.
                let in_air = sim.frames(flight) - 2;
                for w in samples[2..in_air].windows(2) {
                    let speed = (w[1].pos - w[0].pos).with_y(0.0).length() / sim.frame_secs;
                    assert!((speed - THUNDER_WALL_FLING_SPEED).abs() < 0.5, "{fps} fps: {speed} m/s");
                }
                let peak = samples.iter().map(|s| s.pos.y).fold(f32::MIN, f32::max);
                assert!(peak > from.y + 0.3, "{fps} fps: peaked at {peak}");
                let last = samples.last().unwrap().pos;
                let travelled = last.with_y(0.0).distance(from.with_y(0.0));
                assert!((travelled - reach).abs() < 0.4, "{fps} fps: flew {travelled} m, not {reach}");
                let lying = from.y - (ZOMBIE_CAPSULE_LENGTH / 2.0);
                assert!((last.y - lying).abs() < 0.03, "{fps} fps: landed at {}, not {lying}", last.y);
            }
        }

        /// Moving a Turned ally from `Update` was undone whenever the next frame ran a physics
        /// step, so it crept along at a fraction of its sprint. It sprints at its target evenly, at
        /// any frame rate.
        #[test]
        fn a_turned_ally_sprints_at_its_target() {
            for fps in FRAME_RATES {
                let mut sim = Sim::new(fps, movement);
                let ally = sim.spawn(Vec3::new(-4.0, 0.0, 12.0), None);
                sim.spawn(Vec3::new(4.0, 0.0, 12.0), None);
                sim.run(5);
                sim.world_mut().entity_mut(ally).remove::<Zombie>().insert(TurnedAlly::default());
                sim.run(sim.frames(0.2));
                let samples = sim.track(ally, sim.frames(0.4));
                for w in samples.windows(2) {
                    let speed = (w[1].pos - w[0].pos).with_y(0.0).length() / sim.frame_secs;
                    assert!((speed - ZOMBIE_SPRINT_SPEED).abs() < 0.1, "{fps} fps: {speed} m/s");
                }
            }
        }
    }
}
