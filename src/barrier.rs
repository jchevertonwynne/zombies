//! Barriers: the planked windows Zombies tear their way in through.
//!
//! A Zombie that spawns outside a window takes one of its attack spots (or waits outside in a
//! loose group for one), tears a Plank off every so often, swipes the Player through the window
//! when it can reach, and vaults in once the last Plank is gone. The Player holds the Use key to
//! put Planks back for Points, up to a Round cap.

use bevy::prelude::*;

use crate::buy::{Buyable, Focus, Held};
use crate::last_stand::LastStand;
use crate::map::{BarrierId, WINDOWS};
use crate::modifier::{Modifier, Modifiers, Stat, resolve, resolve_points};
use crate::nav::Route;
use crate::player::{Player, PlayerHit};
use crate::points::{Points, PointsAwarded};
use crate::round::{Round, zombie_health};
use crate::state::{GameState, InRun, Pause};
use crate::tuning::*;
use crate::zombie::{Attack, DamageZombie, Dying, HitZone, KnockedDown, Zombie};

/// How far outside the wall a Zombie stands to tear at a window.
const SPOT_OUT: f32 = 0.75;
/// Where along the window each attack spot sits, from its centre.
const SPOT_ALONG: [f32; BARRIER_ATTACK_SPOTS] = [-0.55, 0.0, 0.55];
/// The loose group of waiting Zombies: how far out from the wall, and how far either side of the
/// window's centre.
const WAIT_OUT: std::ops::Range<f32> = 2.4..3.2;
const WAIT_ALONG: f32 = 1.6;
/// How far inside the wall a vault lands, and how high it arcs.
const VAULT_IN: f32 = 1.2;
const VAULT_ARC: f32 = 0.6;
/// How close counts as having reached a spot.
const ARRIVE_DIST: f32 = 0.1;

/// The Plank count of every Barrier. Carpenter restores them all at once.
#[derive(Resource)]
pub struct Barriers {
    planks: [u8; WINDOWS.len()],
}

impl Default for Barriers {
    fn default() -> Self {
        Self {
            planks: [BARRIER_PLANKS; WINDOWS.len()],
        }
    }
}

impl Barriers {
    pub fn planks(&self, barrier: BarrierId) -> u8 {
        self.planks[barrier.0 as usize]
    }

    /// Whether Zombies can climb in: only once every Plank is gone.
    pub fn is_open(&self, barrier: BarrierId) -> bool {
        self.planks(barrier) == 0
    }

    /// Tears one Plank off. False if there was none left.
    pub fn tear(&mut self, barrier: BarrierId) -> bool {
        let planks = &mut self.planks[barrier.0 as usize];
        let torn = *planks > 0;
        *planks = planks.saturating_sub(1);
        torn
    }

    /// Puts one Plank back. False if the Barrier was already whole.
    pub fn rebuild(&mut self, barrier: BarrierId) -> bool {
        let planks = &mut self.planks[barrier.0 as usize];
        let rebuilt = *planks < BARRIER_PLANKS;
        *planks = (*planks + 1).min(BARRIER_PLANKS);
        rebuilt
    }

    /// How many Barriers have no Planks left, for Carpenter's drop rule.
    pub fn fully_broken(&self) -> usize {
        self.planks.iter().filter(|&&planks| planks == 0).count()
    }

    /// Puts every Plank back on every Barrier, as Carpenter does. It pays nothing here and never
    /// touches the rebuild cap.
    pub fn restore_all(&mut self) {
        self.planks = [BARRIER_PLANKS; WINDOWS.len()];
    }
}

/// The Round cap on rebuild Points: `min(50 × round, 500)`.
pub fn rebuild_cap(round: u32) -> u64 {
    (BARRIER_REBUILD_CAP_PER_ROUND.saturating_mul(round as u64)).min(BARRIER_REBUILD_CAP_MAX)
}

/// What one Plank pays and the Round's cap on rebuild Points, through the modifier seam.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlankRates {
    pub points: u64,
    pub cap: u64,
}

impl PlankRates {
    /// Round `round`'s rates from the Player's entries. With none, 10 a Plank up to `rebuild_cap`.
    pub fn resolve(round: u32, entries: &[Modifier]) -> Self {
        Self {
            points: resolve_points(Stat::PlankPoints, BARRIER_REBUILD_POINTS, entries),
            cap: resolve_points(Stat::PlankPointsCap, rebuild_cap(round), entries),
        }
    }
}

/// The Points for putting one Plank back, given the rebuild Points already paid this Round. The
/// cap is checked before each award, counting the award itself, so the real maximum is 490.
pub fn plank_reward(paid_this_round: u64, rates: PlankRates) -> u64 {
    if paid_this_round.saturating_add(rates.points) < rates.cap {
        rates.points
    } else {
        0
    }
}

/// What tearing one Plank off costs the Zombie that tore it: the fraction of its max Health the
/// Player's stat entries give (none in Classic). A normal Zombie's max Health is the Round's.
pub fn plank_tear_damage(round: u32, player: &[Modifier]) -> f32 {
    resolve(Stat::PlankTearDamage, 0.0, player) * zombie_health(round) as f32
}

/// Whether a Zombie at a window can swipe the Player through it: the Player is within
/// `BARRIER_SWIPE_PLAYER_DIST` of the Zombie and `BARRIER_SWIPE_WINDOW_DIST` of the Barrier, on
/// the same floor. Planks don't matter.
pub fn reach_through(player: Vec3, zombie: Vec3, window: Vec3) -> bool {
    let flat = |v: Vec3| Vec2::new(v.x, v.z);
    (player.y - zombie.y).abs() < 1.5
        && flat(player).distance(flat(zombie)) <= BARRIER_SWIPE_PLAYER_DIST
        && flat(player).distance(flat(window)) <= BARRIER_SWIPE_WINDOW_DIST
}

/// What a Zombie at an attack spot does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpotAction {
    /// Swipe at the Player through the window.
    Swipe,
    /// Tear a Plank off.
    Tear,
    /// Climb in: no Planks are left.
    Vault,
    /// Wait for the Barrier to open, as a Crawler does.
    Wait,
}

/// What a Zombie at an attack spot does when its next action comes round: it swipes at a Player
/// it can reach, planked or not, and otherwise tears a Plank, or vaults once none are left. A
/// Crawler never swipes or tears: it waits until the Barrier is open, then vaults.
pub fn spot_action(player_in_reach: bool, planks: u8, crawler: bool) -> SpotAction {
    if planks == 0 && (crawler || !player_in_reach) {
        SpotAction::Vault
    } else if crawler {
        SpotAction::Wait
    } else if player_in_reach {
        SpotAction::Swipe
    } else {
        SpotAction::Tear
    }
}

/// How long climbing in takes: `ZOMBIE_VAULT_SECS`, or `CRAWLER_VAULT_SECS` for a Crawler.
pub fn vault_secs(crawler: bool) -> f32 {
    if crawler { CRAWLER_VAULT_SECS } else { ZOMBIE_VAULT_SECS }
}

/// The Player holding the Use key at a Barrier.
#[derive(Resource, Default)]
pub struct Rebuild {
    /// The Barrier being rebuilt, and seconds into the current Plank.
    holding: Option<(BarrierId, f32)>,
    /// Rebuild Points paid in Round `paid_round`.
    paid: u64,
    paid_round: u32,
}

impl Rebuild {
    /// Advances the hold by `dt`, holding the Use key at `target` (`None` when not holding, or not
    /// at a Barrier missing Planks). True each time a Plank goes back. Letting go, or turning to
    /// another Barrier, throws away the Plank in progress.
    pub fn advance(&mut self, target: Option<BarrierId>, dt: f32) -> bool {
        let Some(target) = target else {
            self.holding = None;
            return false;
        };
        let secs = match self.holding {
            Some((barrier, secs)) if barrier == target => secs + dt,
            _ => dt,
        };
        if secs >= BARRIER_REBUILD_SECS {
            self.holding = Some((target, 0.0));
            true
        } else {
            self.holding = Some((target, secs));
            false
        }
    }

    /// The Points for the Plank just put back in Round `round`, counted against that Round's cap.
    pub fn pay(&mut self, round: u32, rates: PlankRates) -> u64 {
        if round != self.paid_round {
            self.paid_round = round;
            self.paid = 0;
        }
        let reward = plank_reward(self.paid, rates);
        self.paid = self.paid.saturating_add(reward);
        reward
    }
}

/// A Barrier in the Map, on its window. Its Planks are its children.
#[derive(Component)]
pub struct Barrier(pub BarrierId);

/// One Plank on a Barrier, by its place in the stack from the bottom up.
#[derive(Component)]
struct Plank(u8);

/// A Zombie tore a Plank off a Barrier, at `at`.
#[derive(Message)]
pub struct PlankTorn {
    pub at: Vec3,
}

/// The Player put a Plank back.
#[derive(Message)]
pub struct PlankRebuilt;

/// A Zombie outside a window, getting in. While it has this, the Barrier steers it instead of the
/// chase, and the stuck rule leaves it alone.
#[derive(Component, Debug, Clone, Copy)]
pub struct Entering {
    pub barrier: BarrierId,
    stage: Stage,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage {
    /// Waiting outside in the loose group, at its place in it, for an attack spot to free up.
    Waiting(Vec3),
    /// Holding an attack spot: walking to it, then acting each time `next` runs out.
    AtSpot { spot: usize, next: f32 },
    /// Climbing in. A vault that has started always completes.
    Vaulting { elapsed: f32, from: Vec3, to: Vec3 },
}

impl Entering {
    /// A Zombie just spawned outside `barrier`, picking its place in the waiting group.
    pub fn new(barrier: BarrierId) -> Self {
        let (out, along) = window_axes(barrier);
        let place = window_centre(barrier)
            + out * rand::random_range(WAIT_OUT)
            + along * rand::random_range(-WAIT_ALONG..WAIT_ALONG);
        Self {
            barrier,
            stage: Stage::Waiting(place),
        }
    }

    /// Whether it's at a window (not yet climbing in): a Monkey Bomb doesn't pull it away.
    pub fn at_barrier(&self) -> bool {
        !matches!(self.stage, Stage::Vaulting { .. })
    }
}

/// The middle of a window on the wall line, at floor height.
fn window_centre(barrier: BarrierId) -> Vec3 {
    let (_, centre, _) = WINDOWS[barrier.0 as usize];
    Vec3::new(centre.x, 0.0, centre.y)
}

/// The window's way out, and the way along the wall.
fn window_axes(barrier: BarrierId) -> (Vec3, Vec3) {
    let (_, _, out) = WINDOWS[barrier.0 as usize];
    let out = Vec3::new(out.x, 0.0, out.y);
    (out, Vec3::new(-out.z, 0.0, out.x))
}

/// Where a Zombie's feet stand at an attack spot.
fn spot_at(barrier: BarrierId, spot: usize) -> Vec3 {
    let (out, along) = window_axes(barrier);
    window_centre(barrier) + out * SPOT_OUT + along * SPOT_ALONG[spot]
}

pub struct BarrierPlugin;

impl Plugin for BarrierPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Barriers>()
            .init_resource::<Rebuild>()
            .add_message::<PlankTorn>()
            .add_message::<PlankRebuilt>()
            .add_systems(
                OnEnter(GameState::Playing),
                (
                    |mut commands: Commands| {
                        commands.insert_resource(Barriers::default());
                        commands.insert_resource(Rebuild::default());
                    },
                    spawn_barriers,
                ),
            )
            .add_systems(
                FixedUpdate,
                enter_through_windows.run_if(in_state(Pause::Running)),
            )
            .add_systems(
                Update,
                (rebuild_planks, show_planks)
                    .chain()
                    .run_if(in_state(Pause::Running)),
            );
    }
}

fn spawn_barriers(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let plank = meshes.add(Cuboid::new(1.9, 0.18, 0.05));
    let wood = materials.add(Color::srgb(0.45, 0.32, 0.18));
    for i in 0..WINDOWS.len() {
        let barrier = BarrierId(i as u8);
        let (out, _) = window_axes(barrier);
        let centre = window_centre(barrier);
        commands
            .spawn((
                InRun,
                Name::new("Barrier"),
                Barrier(barrier),
                // Its Buyable comes and goes with missing Planks; see `show_planks`.
                Held,
                // Local −Z points out of the window, so local X runs along the wall.
                Transform::from_translation(centre).looking_to(out, Vec3::Y),
                Visibility::default(),
            ))
            .with_children(|window| {
                for p in 0..BARRIER_PLANKS {
                    // Nailed across the outside face, each a little askew.
                    let tilt = if p % 2 == 0 { 0.12 } else { -0.1 };
                    window.spawn((
                        Plank(p),
                        Mesh3d(plank.clone()),
                        MeshMaterial3d(wood.clone()),
                        Transform::from_xyz(0.0, 0.35 + 0.32 * p as f32, -0.27)
                            .with_rotation(Quat::from_rotation_z(tilt)),
                    ));
                }
            });
    }
}

/// Zombies outside a window walk to an attack spot (or wait for one), tear Planks, swipe through
/// the window, and vault in once it's open.
fn enter_through_windows(
    mut commands: Commands,
    // A Zombie knocked down by the Thundergun picks up where it left off once it's back up.
    mut zombies: Query<(Entity, &Zombie, &mut Entering, &mut Transform), (Without<Dying>, Without<KnockedDown>)>,
    player: Single<(&Transform, &Modifiers, Has<LastStand>), (With<Player>, Without<Zombie>)>,
    mut barriers: ResMut<Barriers>,
    mut hits: MessageWriter<PlayerHit>,
    mut torn: MessageWriter<PlankTorn>,
    mut damage: MessageWriter<DamageZombie>,
    round: Res<Round>,
    time: Res<Time>,
) {
    let (player, modifiers, last_stand) = *player;
    let tear_damage = plank_tear_damage(round.number, &modifiers.0);
    let dt = time.delta_secs();
    let mut taken: Vec<(BarrierId, usize)> = zombies
        .iter()
        .filter_map(|(_, _, entering, _)| match entering.stage {
            Stage::AtSpot { spot, .. } => Some((entering.barrier, spot)),
            _ => None,
        })
        .collect();

    for (entity, zombie, mut entering, mut transform) in &mut zombies {
        let barrier = entering.barrier;
        let (out, _) = window_axes(barrier);
        let centre = window_centre(barrier);
        let height = transform.translation.y;

        // A waiting Zombie takes the nearest free spot as soon as there is one.
        if let Stage::Waiting(_) = entering.stage {
            let free = (0..BARRIER_ATTACK_SPOTS)
                .filter(|&spot| !taken.contains(&(barrier, spot)))
                .min_by(|&a, &b| {
                    let d =
                        |spot| spot_at(barrier, spot).distance(transform.translation.with_y(0.0));
                    d(a).total_cmp(&d(b))
                });
            if let Some(spot) = free {
                taken.push((barrier, spot));
                entering.stage = Stage::AtSpot {
                    spot,
                    next: ZOMBIE_PLANK_TEAR_SECS,
                };
            }
        }

        match entering.stage {
            Stage::Waiting(place) => {
                walk_toward(&mut transform, place.with_y(height), zombie.speed() * dt);
            }
            Stage::AtSpot { spot, next } => {
                let at = spot_at(barrier, spot).with_y(height);
                if !walk_toward(&mut transform, at, zombie.speed() * dt) {
                    continue;
                }
                face(&mut transform, -out);
                let in_reach =
                    !last_stand && reach_through(player.translation, transform.translation, centre);
                let planks = barriers.planks(barrier);
                let action = spot_action(in_reach, planks, zombie.is_crawler());
                // An open window is climbed at once, unless there's a Player to swipe at first.
                let next = if action == SpotAction::Vault { 0.0 } else { next - dt };
                if next > 0.0 {
                    entering.stage = Stage::AtSpot { spot, next };
                    continue;
                }
                match action {
                    SpotAction::Swipe => {
                        if rand::random_bool(BARRIER_SWIPE_CHANCE) {
                            hits.write(PlayerHit {
                                damage: ZOMBIE_SWIPE_DAMAGE,
                            });
                        }
                    }
                    SpotAction::Tear => {
                        barriers.tear(barrier);
                        torn.write(PlankTorn {
                            at: centre + Vec3::Y * 1.2,
                        });
                        if tear_damage > 0.0 {
                            damage.write(DamageZombie {
                                zombie: entity,
                                damage: tear_damage,
                                zone: HitZone::Torso,
                                attack: Attack::Barrier,
                            });
                        }
                    }
                    SpotAction::Wait => {}
                    SpotAction::Vault => {
                        taken.retain(|&claim| claim != (barrier, spot));
                        entering.stage = Stage::Vaulting {
                            elapsed: 0.0,
                            from: transform.translation,
                            to: (centre - out * VAULT_IN).with_y(height),
                        };
                        continue;
                    }
                }
                entering.stage = Stage::AtSpot {
                    spot,
                    next: ZOMBIE_PLANK_TEAR_SECS,
                };
            }
            Stage::Vaulting { elapsed, from, to } => {
                let elapsed = elapsed + dt;
                let t = (elapsed / vault_secs(zombie.is_crawler())).min(1.0);
                transform.translation =
                    from.lerp(to, t) + Vec3::Y * VAULT_ARC * (t * std::f32::consts::PI).sin();
                face(&mut transform, -out);
                if t >= 1.0 {
                    // In: from here it hunts the Player like any other Zombie, planning at once.
                    commands
                        .entity(entity)
                        .remove::<Entering>()
                        .insert(Route::default());
                } else {
                    entering.stage = Stage::Vaulting { elapsed, from, to };
                }
            }
        }
    }
}

/// Steps a Zombie up to `step` toward `target`, facing the way it walks. True once it's there.
fn walk_toward(transform: &mut Transform, target: Vec3, step: f32) -> bool {
    let to = target - transform.translation;
    let distance = to.length();
    if distance <= ARRIVE_DIST {
        return true;
    }
    face(transform, to);
    transform.translation += to / distance * step.min(distance);
    false
}

fn face(transform: &mut Transform, direction: Vec3) {
    if direction.x.abs() + direction.z.abs() > 1e-4 {
        transform.rotation = Quat::from_rotation_y(f32::atan2(-direction.x, -direction.z));
    }
}

/// Holding the Use key at a Barrier missing Planks puts them back one at a time, paying Points
/// up to the Round cap. Its Prompt comes from the shared pick, so it never shows while Down.
fn rebuild_planks(
    input: Res<ButtonInput<KeyCode>>,
    focus: Res<Focus>,
    windows: Query<&Barrier>,
    mut barriers: ResMut<Barriers>,
    mut rebuild: ResMut<Rebuild>,
    round: Res<Round>,
    player: Query<&Modifiers, With<Player>>,
    mut points: ResMut<Points>,
    mut awarded: MessageWriter<PointsAwarded>,
    mut rebuilt: MessageWriter<PlankRebuilt>,
    time: Res<Time>,
) {
    let target = focus
        .0
        .filter(|_| input.pressed(KeyCode::KeyF))
        .and_then(|entity| windows.get(entity).ok())
        .map(|window| window.0)
        .filter(|&barrier| barriers.planks(barrier) < BARRIER_PLANKS);
    if !rebuild.advance(target, time.delta_secs()) {
        return;
    }
    let Some(barrier) = target else { return };
    barriers.rebuild(barrier);
    rebuilt.write(PlankRebuilt);
    let entries = player.iter().next().map_or(&[][..], |modifiers| &modifiers.0[..]);
    let reward = rebuild.pay(round.number, PlankRates::resolve(round.number, entries));
    if reward > 0 {
        let paid = points.award(reward);
        awarded.write(PointsAwarded(paid));
    }
}

/// Each Barrier shows the Planks it has left, and offers its Prompt only while missing some.
fn show_planks(
    mut commands: Commands,
    windows: Query<(Entity, &Barrier, Has<Buyable>, &Children)>,
    mut planks: Query<(&Plank, &mut Visibility)>,
    barriers: Res<Barriers>,
) {
    for (entity, window, has_prompt, children) in &windows {
        let left = barriers.planks(window.0);
        for child in children {
            if let Ok((plank, mut visibility)) = planks.get_mut(*child) {
                let shown = if plank.0 < left {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                visibility.set_if_neq(shown);
            }
        }
        let wants_prompt = left < BARRIER_PLANKS;
        if wants_prompt && !has_prompt {
            let (out, _) = window_axes(window.0);
            commands.entity(entity).insert(Buyable {
                // The inside face of the window, about chest height. Used, not bought: no price.
                use_point: window_centre(window.0) - out * 0.3 + Vec3::Y * 1.2,
                prompt: "Hold F to rebuild Barrier".into(),
                price: None,
                range: USE_RANGE,
            });
        } else if !wants_prompt && has_prompt {
            commands.entity(entity).remove::<Buyable>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_barrier_starts_with_six_planks() {
        let barriers = Barriers::default();
        for i in 0..WINDOWS.len() {
            assert_eq!(barriers.planks(BarrierId(i as u8)), 6);
            assert!(!barriers.is_open(BarrierId(i as u8)));
        }
    }

    #[test]
    fn there_are_thirteen_barriers_and_none_in_the_yard() {
        assert_eq!(WINDOWS.len(), 13);
        assert!(
            WINDOWS
                .iter()
                .all(|(zone, ..)| *zone != crate::zone::ZoneId::Yard)
        );
    }

    #[test]
    fn a_barrier_opens_only_once_every_plank_is_gone() {
        let mut barriers = Barriers::default();
        let window = BarrierId(4);
        for _ in 0..5 {
            assert!(barriers.tear(window));
            assert!(!barriers.is_open(window));
        }
        assert!(barriers.tear(window));
        assert!(barriers.is_open(window));
        assert!(!barriers.tear(window), "nothing left to tear");
        assert_eq!(barriers.planks(window), 0);
        assert_eq!(
            barriers.planks(BarrierId(5)),
            6,
            "other Barriers are untouched"
        );
    }

    #[test]
    fn planks_go_back_one_at_a_time_up_to_six() {
        let mut barriers = Barriers::default();
        let window = BarrierId(0);
        assert!(!barriers.rebuild(window), "a whole Barrier takes no more");
        barriers.tear(window);
        barriers.tear(window);
        assert!(barriers.rebuild(window));
        assert_eq!(barriers.planks(window), 5);
    }

    #[test]
    fn carpenter_restores_every_barrier() {
        let mut barriers = Barriers::default();
        for _ in 0..6 {
            barriers.tear(BarrierId(1));
            barriers.tear(BarrierId(2));
        }
        barriers.tear(BarrierId(3));
        assert_eq!(barriers.fully_broken(), 2);
        barriers.restore_all();
        assert_eq!(barriers.fully_broken(), 0);
        assert!((0..13).all(|i| barriers.planks(BarrierId(i)) == 6));
    }

    #[test]
    fn a_tear_entry_costs_the_zombie_a_fraction_of_its_max_health() {
        use crate::modifier::{Effect, Source};
        assert_eq!(plank_tear_damage(1, &[]), 0.0);
        let tenth = [Modifier { stat: Stat::PlankTearDamage, source: Source("A"), effect: Effect::Flat(0.1) }];
        assert!((plank_tear_damage(1, &tenth) - 15.0).abs() < 1e-3);
        assert!((plank_tear_damage(5, &tenth) - 55.0).abs() < 1e-3);
    }

    #[test]
    fn the_rebuild_cap_is_fifty_a_round_up_to_five_hundred() {
        assert_eq!(rebuild_cap(1), 50);
        assert_eq!(rebuild_cap(4), 200);
        assert_eq!(rebuild_cap(10), 500);
        assert_eq!(rebuild_cap(40), 500);
        assert_eq!(rebuild_cap(u32::MAX), 500);
    }

    fn classic(round: u32) -> PlankRates {
        PlankRates::resolve(round, &[])
    }

    #[test]
    fn plank_entries_raise_the_points_and_the_cap() {
        use crate::modifier::{Effect, Source};
        let entries = [
            Modifier { stat: Stat::PlankPoints, source: Source("A"), effect: Effect::Flat(10.0) },
            Modifier { stat: Stat::PlankPointsCap, source: Source("A"), effect: Effect::Mult(2.0) },
        ];
        let rates = PlankRates::resolve(4, &entries);
        assert_eq!(rates, PlankRates { points: 20, cap: 400 });
        assert_eq!(plank_reward(360, rates), 20);
        assert_eq!(plank_reward(380, rates), 0);
    }

    /// The Points a Round pays for putting back `planks` Planks.
    fn paid_for(planks: u32, round: u32) -> u64 {
        let mut rebuild = Rebuild::default();
        (0..planks).map(|_| rebuild.pay(round, classic(round))).sum()
    }

    #[test]
    fn the_cap_is_checked_before_each_award_so_the_real_maximum_is_490() {
        assert_eq!(paid_for(100, 10), 490);
        assert_eq!(paid_for(100, 30), 490);
        assert_eq!(paid_for(100, 1), 40);
        assert_eq!(paid_for(100, 3), 140);
    }

    #[test]
    fn each_plank_pays_ten_until_the_cap() {
        assert_eq!(plank_reward(0, classic(1)), 10);
        assert_eq!(plank_reward(30, classic(1)), 10);
        assert_eq!(plank_reward(40, classic(1)), 0);
        assert_eq!(plank_reward(480, classic(20)), 10);
        assert_eq!(plank_reward(490, classic(20)), 0);
        assert_eq!(paid_for(3, 5), 30);
    }

    #[test]
    fn the_cap_resets_each_round() {
        let mut rebuild = Rebuild::default();
        let round_one: u64 = (0..10).map(|_| rebuild.pay(1, classic(1))).sum();
        assert_eq!(round_one, 40);
        assert_eq!(rebuild.pay(1, classic(1)), 0);
        assert_eq!(rebuild.pay(2, classic(2)), 10);
    }

    #[test]
    fn holding_puts_a_plank_back_each_second() {
        let mut rebuild = Rebuild::default();
        let window = Some(BarrierId(2));
        let mut planks = 0;
        for _ in 0..30 {
            planks += rebuild.advance(window, 0.1) as u32;
        }
        assert!((2..=3).contains(&planks), "{planks}");
    }

    #[test]
    fn letting_go_cancels_the_plank_in_progress() {
        let mut rebuild = Rebuild::default();
        let window = Some(BarrierId(2));
        assert!(!rebuild.advance(window, 0.9));
        assert!(!rebuild.advance(None, 0.016));
        assert!(!rebuild.advance(window, 0.5), "the hold starts over");
        assert!(rebuild.advance(window, 0.5));
    }

    #[test]
    fn turning_to_another_barrier_starts_over() {
        let mut rebuild = Rebuild::default();
        assert!(!rebuild.advance(Some(BarrierId(2)), 0.9));
        assert!(!rebuild.advance(Some(BarrierId(3)), 0.5));
    }

    const WINDOW: Vec3 = Vec3::new(0.0, 0.0, 26.0);

    #[test]
    fn a_zombie_swipes_a_player_close_to_it_and_to_the_window() {
        let zombie = Vec3::new(0.0, 1.0, 26.75);
        assert!(reach_through(Vec3::new(0.0, 1.0, 25.4), zombie, WINDOW));
        // Off to one side but still within 1.3 m of the window.
        assert!(reach_through(Vec3::new(1.0, 1.0, 25.4), zombie, WINDOW));
    }

    #[test]
    fn a_player_far_from_the_window_is_safe() {
        let zombie = Vec3::new(0.0, 1.0, 26.75);
        // 1.4 m from the window, though only 2.15 m from the Zombie.
        assert!(!reach_through(Vec3::new(0.0, 1.0, 24.6), zombie, WINDOW));
    }

    #[test]
    fn a_player_far_from_the_zombie_is_safe() {
        // At the window, but the Zombie is at the far end of the waiting group.
        let zombie = Vec3::new(1.5, 1.0, 28.5);
        let player = Vec3::new(-0.6, 1.0, 25.5);
        assert!(player.with_y(0.0).distance(WINDOW) <= BARRIER_SWIPE_WINDOW_DIST);
        assert!(!reach_through(player, zombie, WINDOW));
    }

    #[test]
    fn a_player_on_another_floor_is_safe() {
        let zombie = Vec3::new(0.0, 1.0, 26.75);
        assert!(!reach_through(Vec3::new(0.0, 5.0, 25.4), zombie, WINDOW));
    }

    #[test]
    fn a_zombie_at_a_window_swipes_planked_or_not_then_tears_then_vaults() {
        assert_eq!(spot_action(true, 6, false), SpotAction::Swipe);
        assert_eq!(spot_action(true, 0, false), SpotAction::Swipe);
        assert_eq!(spot_action(false, 1, false), SpotAction::Tear);
        assert_eq!(spot_action(false, 0, false), SpotAction::Vault);
    }

    #[test]
    fn a_crawler_never_tears_or_reaches_through_and_waits_for_an_open_barrier() {
        for in_reach in [false, true] {
            assert_eq!(spot_action(in_reach, 6, true), SpotAction::Wait);
            assert_eq!(spot_action(in_reach, 1, true), SpotAction::Wait);
            assert_eq!(spot_action(in_reach, 0, true), SpotAction::Vault);
        }
    }

    #[test]
    fn a_crawler_takes_two_seconds_to_vault() {
        assert_eq!(vault_secs(true), 2.0);
        assert_eq!(vault_secs(false), ZOMBIE_VAULT_SECS);
    }

    #[test]
    fn attack_spots_sit_outside_their_window() {
        for i in 0..WINDOWS.len() {
            let barrier = BarrierId(i as u8);
            for spot in 0..BARRIER_ATTACK_SPOTS {
                let at = spot_at(barrier, spot);
                assert_eq!(
                    crate::nav::zone_at(at + Vec3::Y * 0.1),
                    None,
                    "window {i} spot {spot}"
                );
                assert!(at.distance(window_centre(barrier)) < 1.0);
            }
        }
    }
}
