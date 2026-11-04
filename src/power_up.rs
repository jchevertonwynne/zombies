//! Power-ups: Max Ammo, Insta-Kill, Double Points, Nuke, Carpenter and Fire Sale.
//!
//! A drop is armed whenever Points Earned passes a growing threshold, and the next eligible kill
//! drops it; any eligible kill can also drop one by chance. What drops comes from a shuffle bag.
//! A Power-up sits solid, then blinks and vanishes. Timed ones restart if picked up again, and
//! their timers run through the Intermission but freeze on Pause.

use bevy::prelude::*;
use rand::seq::SliceRandom;
use rand::{Rng, RngExt};

use crate::audio::cues::Cue;
use crate::authority::HostSimulation;
use crate::barrier::Barriers;
use crate::door::Door;
use crate::equipment::Equipment;
use crate::last_stand::LastStand;
use crate::map::BOX_LOCATIONS;
use crate::modifier::{Modifiers, Stat, resolve, resolve_count};
use crate::mystery_box::{BoxAssets, BoxRun, BoxState, MysteryBox, spawn_box};
use crate::nav::zone_at;
use crate::player::{LocalPlayer, Player};
use crate::points::{Points, PointsAwarded};
use crate::round::RoundStarted;
use crate::sapper::Sapper;
use crate::state::{GameState, InRun, Pause};
use crate::tuning::*;
use crate::weapon::Loadout;
use crate::zombie::{Attack, DamageZombie, Dying, HitZone, Pay, Zombie, ZombieKilled};
use crate::zone::{FIXED_JOINS, PlayerZone, ZoneId, ZoneSet};

/// The six Power-ups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerUp {
    MaxAmmo,
    InstaKill,
    DoublePoints,
    Nuke,
    Carpenter,
    FireSale,
}

impl PowerUp {
    pub const ALL: [PowerUp; 6] = [
        PowerUp::MaxAmmo,
        PowerUp::InstaKill,
        PowerUp::DoublePoints,
        PowerUp::Nuke,
        PowerUp::Carpenter,
        PowerUp::FireSale,
    ];

    pub fn name(self) -> &'static str {
        match self {
            PowerUp::MaxAmmo => "Max Ammo",
            PowerUp::InstaKill => "Insta-Kill",
            PowerUp::DoublePoints => "Double Points",
            PowerUp::Nuke => "Nuke",
            PowerUp::Carpenter => "Carpenter",
            PowerUp::FireSale => "Fire Sale",
        }
    }

    /// The announcer's line on pickup.
    pub fn announcer(self) -> Cue {
        match self {
            PowerUp::MaxAmmo => Cue::AnnouncerMaxAmmo,
            PowerUp::InstaKill => Cue::AnnouncerInstaKill,
            PowerUp::DoublePoints => Cue::AnnouncerDoublePoints,
            PowerUp::Nuke => Cue::AnnouncerNuke,
            PowerUp::Carpenter => Cue::AnnouncerCarpenter,
            PowerUp::FireSale => Cue::AnnouncerFireSale,
        }
    }

    pub fn colour(self) -> Color {
        match self {
            PowerUp::MaxAmmo => Color::srgb(0.3, 0.9, 0.3),
            PowerUp::InstaKill => Color::srgb(0.9, 0.9, 0.9),
            PowerUp::DoublePoints => Color::srgb(1.0, 0.8, 0.2),
            PowerUp::Nuke => Color::srgb(1.0, 0.35, 0.1),
            PowerUp::Carpenter => Color::srgb(0.6, 0.4, 0.2),
            PowerUp::FireSale => Color::srgb(0.9, 0.15, 0.15),
        }
    }
}

/// Seconds left on each timed Power-up; zero when off.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct ActivePowerUps {
    insta_kill: f32,
    double_points: f32,
    fire_sale: f32,
    /// Seconds added to every timed Power-up, resolved from the Player's stat entries.
    pub extra_secs: f32,
}

impl ActivePowerUps {
    /// The timed Power-ups, in the order their countdowns show.
    pub const TIMED: [PowerUp; 3] = [PowerUp::InstaKill, PowerUp::DoublePoints, PowerUp::FireSale];

    fn timer_mut(&mut self, power_up: PowerUp) -> Option<&mut f32> {
        match power_up {
            PowerUp::InstaKill => Some(&mut self.insta_kill),
            PowerUp::DoublePoints => Some(&mut self.double_points),
            PowerUp::FireSale => Some(&mut self.fire_sale),
            _ => None,
        }
    }

    /// Seconds left on a timed Power-up; always zero for an instant one.
    pub fn left(&self, power_up: PowerUp) -> f32 {
        match power_up {
            PowerUp::InstaKill => self.insta_kill,
            PowerUp::DoublePoints => self.double_points,
            PowerUp::FireSale => self.fire_sale,
            _ => 0.0,
        }
    }

    pub fn active(&self, power_up: PowerUp) -> bool {
        self.left(power_up) > 0.0
    }

    /// Starts a timed Power-up, or restarts it if it's already on. Instant ones have no timer.
    pub fn start(&mut self, power_up: PowerUp) {
        let secs = POWER_UP_TIMED_SECS + self.extra_secs;
        if let Some(timer) = self.timer_mut(power_up) {
            *timer = secs;
        }
    }

    /// Runs every timer down by `dt`.
    pub fn tick(&mut self, dt: f32) {
        for timer in [&mut self.insta_kill, &mut self.double_points, &mut self.fire_sale] {
            *timer = (*timer - dt).max(0.0);
        }
    }
}

/// When the next drop comes: the Points Earned threshold, and the drops this Round.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct DropMeter {
    /// The Points Earned that arms the next drop.
    pub threshold: f64,
    /// The gap that set it, which grows with each drop armed.
    pub gap: f64,
    /// Whether a drop waits for the next eligible death.
    pub armed: bool,
    /// Drops so far this Round.
    pub this_round: u32,
    /// The most drops a Round, resolved from the Player's stat entries.
    pub per_round: u32,
    /// The chance any eligible kill drops one, resolved from the Player's stat entries.
    pub chance: f64,
}

impl Default for DropMeter {
    fn default() -> Self {
        Self {
            threshold: POWER_UP_THRESHOLD_START,
            gap: POWER_UP_THRESHOLD_START,
            armed: false,
            this_round: 0,
            per_round: POWER_UPS_PER_ROUND,
            chance: POWER_UP_KILL_CHANCE,
        }
    }
}

impl DropMeter {
    /// Arms a drop once Points Earned passes the threshold. The gap grows ×1.14 and the next
    /// threshold is counted from the Points Earned now.
    pub fn watch(&mut self, earned: u64) {
        if earned as f64 > self.threshold {
            self.armed = true;
            self.gap *= POWER_UP_THRESHOLD_GROWTH;
            self.threshold = earned as f64 + self.gap;
        }
    }

    /// Whether a kill drops a Power-up. `eligible` is false for an Unpaid Kill (the Nuke's
    /// included), `chance` is uniform in [0, 1), and `reachable` says whether the Player can get
    /// to where it died. A drop disarms the meter; a drop that can't land leaves it armed.
    pub fn roll(&mut self, eligible: bool, chance: f64, reachable: bool) -> bool {
        if !eligible || self.this_round >= self.per_round {
            return false;
        }
        if !(self.armed || chance < self.chance) || !reachable {
            return false;
        }
        self.armed = false;
        self.this_round += 1;
        true
    }

    pub fn new_round(&mut self) {
        self.this_round = 0;
    }
}

/// What the bag's skip rules look at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BagRules {
    /// Barriers with no Planks left.
    pub broken_barriers: usize,
    /// How many times the Mystery Box has moved this Run.
    pub box_moves: u32,
    pub fire_sale_active: bool,
    /// Whether Max Ammo is in the bag at all. It always is in Classic.
    pub max_ammo: bool,
}

/// Whether a Power-up may come out of the bag now: Carpenter needs 5 broken Barriers, Fire Sale
/// needs the Box to have moved once and no Fire Sale already on, and Max Ammo can be left out.
pub fn may_drop(power_up: PowerUp, rules: BagRules) -> bool {
    match power_up {
        PowerUp::MaxAmmo => rules.max_ammo,
        PowerUp::Carpenter => rules.broken_barriers >= CARPENTER_MIN_BROKEN_BARRIERS,
        PowerUp::FireSale => rules.box_moves > 0 && !rules.fire_sale_active,
        _ => true,
    }
}

/// The next Power-up from the shuffle bag. A skipped one is used up, and an empty bag is refilled
/// with all six in a fresh order. Insta-Kill is never skipped, so this always ends.
pub fn next_power_up<R: Rng + ?Sized>(bag: &mut Vec<PowerUp>, rules: BagRules, rng: &mut R) -> PowerUp {
    loop {
        if bag.is_empty() {
            *bag = PowerUp::ALL.to_vec();
            bag.shuffle(rng);
        }
        let power_up = bag.remove(0);
        if may_drop(power_up, rules) {
            return power_up;
        }
    }
}

/// Whether a dropped Power-up shows, `age` seconds after it dropped: solid, then blinking, then
/// `None` once it's gone.
pub fn drop_shown(age: f32) -> Option<bool> {
    if age >= POWER_UP_SOLID_SECS + POWER_UP_BLINK_DROP_SECS {
        None
    } else if age < POWER_UP_SOLID_SECS {
        Some(true)
    } else {
        Some((((age - POWER_UP_SOLID_SECS) / POWER_UP_BLINK_INTERVAL_SECS) as u32).is_multiple_of(2))
    }
}

/// Every Zone the Player can walk to from `from` through the joins open now.
pub fn reachable_zones(from: ZoneId, joins: &[(ZoneId, ZoneId)]) -> ZoneSet {
    let mut reached = ZoneSet::default();
    reached.insert(from);
    let mut grew = true;
    while grew {
        grew = false;
        for &(a, b) in joins {
            if reached.contains(a) != reached.contains(b) {
                reached.insert(a);
                reached.insert(b);
                grew = true;
            }
        }
    }
    reached
}

/// A Power-up was taken.
#[derive(Message, Debug, Clone, Copy)]
pub struct PowerUpTaken(pub PowerUp);

/// Gives the Player a Power-up straight away, as if they'd walked into it.
#[derive(Message, Debug, Clone, Copy)]
pub struct GivePowerUp(pub PowerUp);

/// Drops a Power-up at `at`, outside the per-Round cap and needing no threshold: `power_up`, or
/// the bag's next one if `None`.
#[derive(Message, Debug, Clone, Copy)]
pub struct DropPowerUp {
    pub at: Vec3,
    pub power_up: Option<PowerUp>,
}

/// The shuffle bag, next first.
#[derive(Resource, Default)]
struct Bag(Vec<PowerUp>);

/// A Power-up on the ground.
#[derive(Component)]
pub struct Dropped {
    power_up: PowerUp,
    age: f32,
    /// Where it floats.
    at: Vec3,
}

/// A Zombie the Nuke has caught, dying when its moment is up.
#[derive(Component)]
struct Nuked {
    /// The Player who took the Nuke.
    player: Entity,
    left: f32,
}

/// A Box Fire Sale added. It goes once the Fire Sale is over and it's done with any spin.
#[derive(Component)]
struct FireSaleBox;


#[derive(Resource)]
struct PowerUpAssets {
    mesh: Handle<Mesh>,
    materials: Vec<Handle<StandardMaterial>>,
}

impl FromWorld for PowerUpAssets {
    fn from_world(world: &mut World) -> Self {
        let mesh = world.resource_mut::<Assets<Mesh>>().add(Cuboid::from_length(0.4));
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let materials = PowerUp::ALL
            .iter()
            .map(|power_up| {
                let colour = power_up.colour();
                materials.add(StandardMaterial {
                    base_color: colour,
                    emissive: LinearRgba::from(colour) * 4.0,
                    ..default()
                })
            })
            .collect();
        Self { mesh, materials }
    }
}

pub struct PowerUpPlugin;

impl Plugin for PowerUpPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActivePowerUps>()
            .init_resource::<DropMeter>()
            .init_resource::<Bag>()
            .init_resource::<PowerUpAssets>()
            .add_message::<PowerUpTaken>()
            .add_message::<DropPowerUp>()
            .add_message::<GivePowerUp>()
            .add_systems(OnEnter(GameState::Playing), reset)
            .add_systems(
                Update,
                (resolve_stats, tick_timers, drop_power_ups, age_drops, pick_up, nuke, fire_sale)
                    .chain()
                    .in_set(HostSimulation)
                    .run_if(in_state(Pause::Running)),
            );
    }
}

fn reset(mut active: ResMut<ActivePowerUps>, mut meter: ResMut<DropMeter>, mut bag: ResMut<Bag>) {
    *active = ActivePowerUps::default();
    *meter = DropMeter::default();
    bag.0.clear();
}

/// Timed Power-ups run down, and Double Points follows its timer.
/// Timed Power-up length and the per-Round cap follow the Player's stat entries.
fn resolve_stats(
    player: Single<&Modifiers, With<LocalPlayer>>,
    mut active: ResMut<ActivePowerUps>,
    mut meter: ResMut<DropMeter>,
) {
    let extra = resolve(Stat::PowerUpDuration, 0.0, &player.0);
    if active.extra_secs != extra {
        active.extra_secs = extra;
    }
    let per_round = resolve_count(Stat::PowerUpsPerRound, POWER_UPS_PER_ROUND, &player.0);
    if meter.per_round != per_round {
        meter.per_round = per_round;
    }
    let chance = POWER_UP_KILL_CHANCE * f64::from(resolve(Stat::PowerUpDropChance, 1.0, &player.0));
    if meter.chance != chance {
        meter.chance = chance;
    }
}

fn tick_timers(mut active: ResMut<ActivePowerUps>, mut players: Query<&mut Points, With<Player>>, time: Res<Time>) {
    active.tick(time.delta_secs());
    let double = active.active(PowerUp::DoublePoints);
    for mut points in &mut players {
        if points.double != double {
            points.double = double;
        }
    }
}

/// Arms drops from Points Earned, rolls each kill, and drops what's asked for outside the cap.
fn drop_power_ups(
    mut commands: Commands,
    mut kills: MessageReader<ZombieKilled>,
    mut requests: MessageReader<DropPowerUp>,
    mut rounds: MessageReader<RoundStarted>,
    mut meter: ResMut<DropMeter>,
    mut bag: ResMut<Bag>,
    points: Query<&Points, With<Player>>,
    zones: Query<&PlayerZone, With<Player>>,
    doors: Query<&Door>,
    barriers: Res<Barriers>,
    box_run: Res<BoxRun>,
    active: Res<ActivePowerUps>,
    assets: Res<PowerUpAssets>,
    player: Single<&Modifiers, With<LocalPlayer>>,
) {
    if rounds.read().count() > 0 {
        meter.new_round();
    }
    meter.watch(points.iter().map(|points| points.earned).sum());

    let joins: Vec<_> = FIXED_JOINS
        .into_iter()
        .chain(doors.iter().filter(|door| door.open).filter_map(|door| door.joins))
        .collect();
    let reachable = zones
        .iter()
        .fold(ZoneSet::default(), |set, zone| set.union(reachable_zones(zone.0, &joins)));
    let rules = BagRules {
        broken_barriers: barriers.fully_broken(),
        box_moves: box_run.moves,
        fire_sale_active: active.active(PowerUp::FireSale),
        max_ammo: resolve(Stat::MaxAmmoInBag, 1.0, &player.0) >= 1.0,
    };
    let rng = &mut rand::rng();
    let mut drops: Vec<(Vec3, Option<PowerUp>)> = requests.read().map(|request| (request.at, request.power_up)).collect();
    for kill in kills.read() {
        // A point just above its feet, so the floor it stood on is inside its Zone.
        let reached = zone_at(kill.at + Vec3::Y * 0.5).is_some_and(|zone| reachable.contains(zone));
        if meter.roll(kill.pay == Pay::Paid, rng.random::<f64>(), reached) {
            drops.push((kill.at, None));
        }
    }
    for (at, power_up) in drops {
        let power_up = power_up.unwrap_or_else(|| next_power_up(&mut bag.0, rules, rng));
        let at = at + Vec3::Y * POWER_UP_FLOAT_HEIGHT;
        commands.spawn((
            InRun,
            Name::new(power_up.name()),
            Dropped { power_up, age: 0.0, at },
            Mesh3d(assets.mesh.clone()),
            MeshMaterial3d(assets.materials[power_up as usize].clone()),
            Transform::from_translation(at),
            Visibility::default(),
            children![(
                PointLight {
                    color: Color::srgb(0.4, 1.0, 0.4),
                    intensity: 40_000.0,
                    range: 4.0,
                    ..default()
                },
                Transform::default(),
            )],
        ));
    }
}

/// A Power-up on the ground spins and bobs, blinks near the end, then vanishes.
fn age_drops(
    mut commands: Commands,
    mut drops: Query<(Entity, &mut Dropped, &mut Transform, &mut Visibility)>,
    time: Res<Time>,
) {
    for (entity, mut dropped, mut transform, mut visibility) in &mut drops {
        dropped.age += time.delta_secs();
        let Some(shown) = drop_shown(dropped.age) else {
            commands.entity(entity).despawn();
            continue;
        };
        visibility.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
        transform.translation = dropped.at + Vec3::Y * (dropped.age * 2.5).sin() * 0.08;
        transform.rotation = Quat::from_rotation_y(dropped.age * 1.5);
    }
}

/// Walking into a Power-up takes it, and so does being given one.
fn pick_up(
    mut commands: Commands,
    drops: Query<(Entity, &Dropped)>,
    mut given: MessageReader<GivePowerUp>,
    player: Single<(Entity, &Transform, &mut Loadout, &mut Equipment, &mut Points, Has<LastStand>), With<LocalPlayer>>,
    // The Sapper survives the Nuke.
    zombies: Query<Entity, (With<Zombie>, Without<Dying>, Without<Nuked>, Without<Sapper>)>,
    mut active: ResMut<ActivePowerUps>,
    mut barriers: ResMut<Barriers>,
    mut awarded: MessageWriter<PointsAwarded>,
    mut taken: MessageWriter<PowerUpTaken>,
) {
    let (player, transform, mut loadout, mut equipment, mut points, last_stand) = player.into_inner();
    let feet = transform.translation;
    let mut takes: Vec<PowerUp> = given.read().map(|GivePowerUp(power_up)| *power_up).collect();
    for (entity, dropped) in &drops {
        let offset = dropped.at - feet;
        if Vec2::new(offset.x, offset.z).length() > POWER_UP_PICKUP_RADIUS || offset.y.abs() > 2.0 {
            continue;
        }
        commands.entity(entity).despawn();
        takes.push(dropped.power_up);
    }
    for power_up in takes {
        taken.write(PowerUpTaken(power_up));
        active.start(power_up);
        let bonus = match power_up {
            PowerUp::MaxAmmo => {
                loadout.max_ammo();
                equipment.max_ammo();
                0
            }
            PowerUp::Nuke => {
                let rng = &mut rand::rng();
                for zombie in &zombies {
                    commands.entity(zombie).insert(Nuked {
                        player,
                        left: rng.random_range(NUKE_DELAY_SECS),
                    });
                }
                NUKE_POINTS
            }
            PowerUp::Carpenter => {
                barriers.restore_all();
                CARPENTER_POINTS
            }
            PowerUp::DoublePoints => {
                points.double = true;
                0
            }
            PowerUp::InstaKill | PowerUp::FireSale => 0,
        };
        // A flat bonus, which Last Stand earns nothing of.
        if bonus > 0 && !last_stand {
            let paid = points.award(bonus);
            awarded.write(PointsAwarded { player, amount: paid });
        }
    }
}

/// Each Zombie the Nuke caught dies when its moment comes: an Unpaid Kill.
fn nuke(
    mut commands: Commands,
    mut zombies: Query<(Entity, &mut Nuked), Without<Dying>>,
    mut damage: MessageWriter<DamageZombie>,
    time: Res<Time>,
) {
    for (entity, mut nuked) in &mut zombies {
        nuked.left -= time.delta_secs();
        if nuked.left <= 0.0 {
            damage.write(DamageZombie {
                player: Some(nuked.player),
                zombie: entity,
                damage: f32::INFINITY,
                zone: HitZone::Torso,
                attack: Attack::Nuke,
            });
            commands.entity(entity).remove::<Nuked>();
        }
    }
}

/// Fire Sale: every Box costs 10 and none gives a Teddy, and a Box stands at every Box Location.
/// Once it's over, the added Boxes go as soon as they're done with any spin, and a Box that was
/// moving finishes its move only after that.
fn fire_sale(
    mut commands: Commands,
    active: Res<ActivePowerUps>,
    assets: Res<BoxAssets>,
    mut boxes: Query<(Entity, &mut MysteryBox, Has<FireSaleBox>)>,
    mut run: ResMut<BoxRun>,
) {
    let on = active.active(PowerUp::FireSale);
    let price = if on { FIRE_SALE_BOX_PRICE } else { BOX_PRICE };
    let mut occupied = [false; BOX_LOCATIONS.len()];
    let mut extras = 0;
    for (entity, mut mystery_box, added) in &mut boxes {
        if added && !on && matches!(mystery_box.state, BoxState::Idle | BoxState::Rearming { .. }) {
            commands.entity(entity).despawn();
            continue;
        }
        if mystery_box.price != price {
            mystery_box.price = price;
        }
        // Only the Run's own Box ever gives Teddies; a spin already going keeps what it had.
        if !added && mystery_box.teddy == on {
            mystery_box.teddy = !on;
        }
        if mystery_box.present() {
            occupied[mystery_box.location] = true;
        }
        extras += added as u32;
    }
    if on {
        for location in (0..BOX_LOCATIONS.len()).filter(|&location| !occupied[location]) {
            let added = spawn_box(&mut commands, &assets, location, FIRE_SALE_BOX_PRICE, false);
            commands.entity(added).insert(FireSaleBox);
            extras += 1;
        }
    }
    let hold = on || extras > 0;
    if run.hold_moves != hold {
        run.hold_moves = hold;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    const NO_CHANCE: f64 = 0.99;

    fn rules(broken_barriers: usize, box_moves: u32, fire_sale_active: bool) -> BagRules {
        BagRules {
            broken_barriers,
            box_moves,
            fire_sale_active,
            max_ammo: true,
        }
    }

    #[test]
    fn the_first_drop_arms_after_2000_points_earned() {
        let mut meter = DropMeter::default();
        meter.watch(2000);
        assert!(!meter.armed);
        meter.watch(2010);
        assert!(meter.armed);
    }

    #[test]
    fn each_gap_grows_fourteen_percent_from_where_it_armed() {
        let mut meter = DropMeter::default();
        meter.watch(2010);
        assert!((meter.gap - 2280.0).abs() < 1e-9);
        assert!((meter.threshold - 4290.0).abs() < 1e-9);
        meter.armed = false;
        meter.watch(4290);
        assert!(!meter.armed);
        meter.watch(4300);
        assert!(meter.armed);
        assert!((meter.gap - 2280.0 * 1.14).abs() < 1e-9);
        assert!((meter.threshold - (4300.0 + 2280.0 * 1.14)).abs() < 1e-9);
    }

    #[test]
    fn an_armed_drop_falls_from_the_next_eligible_death_only() {
        let mut meter = DropMeter {
            armed: true,
            ..DropMeter::default()
        };
        // Unpaid Kills, the Nuke's among them, never drop and leave it armed.
        assert!(!meter.roll(false, NO_CHANCE, true));
        assert!(meter.armed);
        // Nor does a death where the Player can't reach.
        assert!(!meter.roll(true, NO_CHANCE, false));
        assert!(meter.armed);
        assert!(meter.roll(true, NO_CHANCE, true));
        assert!(!meter.armed);
        assert!(!meter.roll(true, NO_CHANCE, true));
    }

    #[test]
    fn any_eligible_kill_has_a_three_percent_chance() {
        let mut meter = DropMeter::default();
        assert!(!meter.roll(true, 0.031, true));
        assert!(meter.roll(true, 0.029, true));
        assert!(!meter.roll(false, 0.0, true));
    }

    #[test]
    fn a_halved_chance_halves_the_kill_chance() {
        let mut meter = DropMeter { chance: POWER_UP_KILL_CHANCE * 0.5, ..DropMeter::default() };
        assert!(!meter.roll(true, 0.016, true));
        assert!(meter.roll(true, 0.014, true));
    }

    #[test]
    fn max_ammo_can_leave_the_bag() {
        let rng = &mut StdRng::seed_from_u64(7);
        let rules = BagRules { max_ammo: false, ..rules(10, 1, false) };
        assert!(!may_drop(PowerUp::MaxAmmo, rules));
        let mut bag = Vec::new();
        for _ in 0..60 {
            assert_ne!(next_power_up(&mut bag, rules, rng), PowerUp::MaxAmmo);
        }
    }

    #[test]
    fn a_raised_cap_allows_more_drops_a_round() {
        let mut meter = DropMeter { per_round: 5, ..DropMeter::default() };
        for _ in 0..5 {
            assert!(meter.roll(true, 0.0, true));
        }
        assert!(!meter.roll(true, 0.0, true));
    }

    #[test]
    fn extra_seconds_lengthen_timed_power_ups() {
        let mut active = ActivePowerUps { extra_secs: 10.0, ..ActivePowerUps::default() };
        active.start(PowerUp::DoublePoints);
        assert_eq!(active.left(PowerUp::DoublePoints), POWER_UP_TIMED_SECS + 10.0);
    }

    #[test]
    fn at_most_four_drops_a_round() {
        let mut meter = DropMeter::default();
        for _ in 0..4 {
            assert!(meter.roll(true, 0.0, true));
        }
        meter.armed = true;
        assert!(!meter.roll(true, 0.0, true));
        meter.new_round();
        assert!(meter.roll(true, 0.0, true));
    }

    #[test]
    fn the_bag_gives_all_six_before_reshuffling() {
        let rng = &mut StdRng::seed_from_u64(3);
        let mut bag = Vec::new();
        let open = rules(13, 1, false);
        for _ in 0..3 {
            let mut drawn: Vec<_> = (0..6).map(|_| next_power_up(&mut bag, open, rng)).collect();
            drawn.sort_by_key(|&power_up| power_up as usize);
            assert_eq!(drawn, PowerUp::ALL);
        }
    }

    #[test]
    fn carpenter_needs_five_broken_barriers() {
        assert!(!may_drop(PowerUp::Carpenter, rules(4, 1, false)));
        assert!(may_drop(PowerUp::Carpenter, rules(5, 1, false)));
    }

    #[test]
    fn fire_sale_waits_for_the_box_to_move_and_never_doubles_up() {
        assert!(!may_drop(PowerUp::FireSale, rules(13, 0, false)));
        assert!(may_drop(PowerUp::FireSale, rules(13, 1, false)));
        assert!(!may_drop(PowerUp::FireSale, rules(13, 3, true)));
    }

    #[test]
    fn skipped_power_ups_are_used_up() {
        let rng = &mut StdRng::seed_from_u64(1);
        let mut bag = vec![PowerUp::Carpenter, PowerUp::FireSale, PowerUp::Nuke];
        assert_eq!(next_power_up(&mut bag, rules(0, 0, false), rng), PowerUp::Nuke);
        assert!(bag.is_empty());
        for _ in 0..200 {
            let drawn = next_power_up(&mut bag, rules(0, 0, false), rng);
            assert!(!matches!(drawn, PowerUp::Carpenter | PowerUp::FireSale));
        }
    }

    #[test]
    fn solid_for_fifteen_seconds_then_blinks_for_eleven_and_a_half() {
        assert_eq!(drop_shown(0.0), Some(true));
        assert_eq!(drop_shown(14.9), Some(true));
        assert_eq!(drop_shown(15.1), Some(true));
        assert_eq!(drop_shown(15.3), Some(false));
        assert_eq!(drop_shown(15.6), Some(true));
        assert_eq!(drop_shown(26.4), Some(false));
        assert_eq!(drop_shown(26.5), None);
    }

    #[test]
    fn picking_up_an_active_timed_power_up_restarts_it() {
        let mut active = ActivePowerUps::default();
        active.start(PowerUp::InstaKill);
        active.tick(20.0);
        assert_eq!(active.left(PowerUp::InstaKill), 10.0);
        active.start(PowerUp::InstaKill);
        assert_eq!(active.left(PowerUp::InstaKill), 30.0);
        active.tick(31.0);
        assert!(!active.active(PowerUp::InstaKill));
        // Instant Power-ups have no timer.
        active.start(PowerUp::Nuke);
        assert!(!active.active(PowerUp::Nuke));
    }

    #[test]
    fn drops_land_only_in_zones_the_player_can_reach() {
        let start_only = reachable_zones(ZoneId::StartRoom, &FIXED_JOINS);
        assert_eq!(start_only.iter().collect::<Vec<_>>(), [ZoneId::StartRoom]);

        let joins = [FIXED_JOINS[0], FIXED_JOINS[1], (ZoneId::StartRoom, ZoneId::WestHall)];
        let reached = reachable_zones(ZoneId::StartRoom, &joins);
        for zone in [ZoneId::StartRoom, ZoneId::WestHall, ZoneId::Balcony, ZoneId::Yard] {
            assert!(reached.contains(zone));
        }
        assert!(!reached.contains(ZoneId::EastHall));
    }
}
