//! Barriers: the planked windows Zombies tear their way in through.
//!
//! A Zombie that spawns outside a window takes one of its attack spots (or waits outside in a
//! loose group for one), tears a Plank off every so often, swipes the Player through the window
//! when it can reach, and vaults in once the last Plank is gone. The Player holds the Use key to
//! put Planks back for Points, up to a Round cap.

use bevy::prelude::*;

use crate::authority::HostSimulation;
use crate::controls::{Action, Controls};
use crate::audio::PlayCue;
use crate::audio::cues::Cue;
use crate::buy::{Buyable, Focus, Held, Nearby, UseRequest};
use crate::map::{BarrierId, WINDOWS};
use crate::modifier::{Modifier, Modifiers, Stat, resolve, resolve_points};
use crate::nav::Route;
use crate::player::{LocalPlayer, Player, PlayerHit};
use crate::target::Players;
use crate::points::{Points, PointsAwarded};
use crate::round::{Round, zombie_health};
use crate::state::{GameState, InRun, world_runs};
use crate::tuning::*;
use crate::zombie::{Attack, DamageZombie, Dying, HitZone, KnockedDown, Stunned, Zombie};

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
/// The fastest the rebuild hand's push moves, in whole pushes per second.
const REBUILD_PUSH_RATE: f32 = 8.0;
/// How long a torn-off Plank lies on the ground before it goes.
const TORN_PLANK_SECS: f32 = 2.5;

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

    /// Tears every Plank off at once, as the Sapper does. Returns how many there were.
    pub fn smash(&mut self, barrier: BarrierId) -> u8 {
        std::mem::take(&mut self.planks[barrier.0 as usize])
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
    /// For the view model's hand: how far up it is (0 to 1, eased in and out of a hold), and how
    /// far it's pushed forward (0 to 1, following the Plank in progress).
    hand: f32,
    reach: f32,
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

    /// Eases the hand up while a hold goes on and away when it doesn't, and has it push once per
    /// Plank, `dt` seconds on. Call it after `advance`.
    pub fn animate(&mut self, dt: f32) {
        let holding = self.holding.is_some();
        let step = dt / REBUILD_HAND_SECS;
        self.hand = if holding { (self.hand + step).min(1.0) } else { (self.hand - step).max(0.0) };
        let push = match self.holding {
            Some((_, secs)) => crate::hands::rebuild_push(secs / BARRIER_REBUILD_SECS),
            None => 0.0,
        };
        // Never faster than the push itself, so letting go or a new Plank doesn't jerk it.
        let max_move = dt * REBUILD_PUSH_RATE;
        self.reach += (push - self.reach).clamp(-max_move, max_move);
    }

    /// The hand's state for the view model: how far up it is, and how far it's pushed.
    pub fn hand(&self) -> (f32, f32) {
        (self.hand, self.reach)
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

/// A Zombie tore a Plank off a Barrier, at `at`: the `plank`th from the bottom.
#[derive(Message)]
pub struct PlankTorn {
    pub at: Vec3,
    pub barrier: BarrierId,
    pub plank: u8,
}

/// A Plank just put back, sliding into its slot from inside the room.
#[derive(Component)]
struct Sliding(f32);

/// A Plank's look, shared by the Barriers and the Planks torn off them.
#[derive(Resource)]
struct PlankLook {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

/// A Plank a Zombie tore off, tossed aside and left lying on the ground a moment.
#[derive(Component)]
struct TornPlank {
    velocity: Vec3,
    spin: Vec3,
    age: f32,
}

/// The Player put a Plank back, at `at`.
#[derive(Message)]
pub struct PlankRebuilt {
    pub at: Vec3,
}

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

    /// What it's doing at the window, for its animation.
    pub fn at_window(&self) -> AtWindow {
        match self.stage {
            Stage::Waiting(_) => AtWindow::Approaching,
            // Still walking to its spot: its clock hasn't started.
            Stage::AtSpot { next, .. } if next >= ZOMBIE_PLANK_TEAR_SECS => AtWindow::Approaching,
            Stage::AtSpot { next, .. } => AtWindow::Working {
                cycle: 1.0 - next / ZOMBIE_PLANK_TEAR_SECS,
            },
            Stage::Vaulting { .. } => AtWindow::Vaulting,
        }
    }
}

/// What a Zombie getting in through a window is doing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AtWindow {
    /// Waiting its turn, or walking to its attack spot.
    Approaching,
    /// At its spot, `cycle` of the way (0 to 1) to its next tear or swipe.
    Working { cycle: f32 },
    /// Climbing in.
    Vaulting,
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

/// Where the `p`th Plank from the bottom is nailed, on its Barrier (whose local −Z points out of
/// the window, so local X runs along the wall): across the outside face, each a little askew.
fn plank_transform(p: u8) -> Transform {
    let tilt = if p.is_multiple_of(2) { 0.12 } else { -0.1 };
    Transform::from_xyz(0.0, 0.35 + 0.32 * p as f32, -0.27).with_rotation(Quat::from_rotation_z(tilt))
}

/// A Barrier's own Transform, on its window.
fn barrier_transform(barrier: BarrierId) -> Transform {
    let (out, _) = window_axes(barrier);
    Transform::from_translation(window_centre(barrier)).looking_to(out, Vec3::Y)
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
                enter_through_windows.in_set(HostSimulation).run_if(world_runs),
            )
            .add_systems(
                Update,
                ((rebuild_planks, rebuild_barriers.in_set(HostSimulation), show_planks, slide_planks).chain(), (toss_torn_planks, fly_torn_planks).chain())
                    .run_if(world_runs),
            );
    }
}

fn spawn_barriers(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let plank = meshes.add(Cuboid::new(1.9, 0.18, 0.05));
    let wood = materials.add(StandardMaterial {
        base_color: Color::srgb(0.45, 0.32, 0.18),
        perceptual_roughness: 0.9,
        ..default()
    });
    for i in 0..WINDOWS.len() {
        let barrier = BarrierId(i as u8);
        commands
            .spawn((
                InRun,
                Name::new("Barrier"),
                Barrier(barrier),
                // Its Buyable comes and goes with missing Planks; see `show_planks`.
                Held,
                Nearby,
                barrier_transform(barrier),
                Visibility::default(),
            ))
            .with_children(|window| {
                for p in 0..BARRIER_PLANKS {
                    window.spawn((
                        Plank(p),
                        Mesh3d(plank.clone()),
                        MeshMaterial3d(wood.clone()),
                        plank_transform(p),
                    ));
                }
            });
    }
    commands.insert_resource(PlankLook {
        mesh: plank,
        material: wood,
    });
}

/// A torn-off Plank comes away in the Zombie's hands and is flung aside, outside the window.
fn toss_torn_planks(mut commands: Commands, mut torn: MessageReader<PlankTorn>, look: Option<Res<PlankLook>>) {
    let Some(look) = look else {
        torn.clear();
        return;
    };
    for plank in torn.read() {
        let (out, along) = window_axes(plank.barrier);
        let from = barrier_transform(plank.barrier) * plank_transform(plank.plank);
        let aside = if rand::random_bool(0.5) { 1.0 } else { -1.0 };
        commands.spawn((
            InRun,
            Name::new("Torn Plank"),
            TornPlank {
                velocity: out * rand::random_range(2.0..3.5)
                    + along * aside * rand::random_range(0.5..2.0)
                    + Vec3::Y * rand::random_range(1.5..3.0),
                spin: Vec3::new(
                    rand::random_range(-6.0..6.0),
                    rand::random_range(-4.0..4.0),
                    rand::random_range(-8.0..8.0),
                ),
                age: 0.0,
            },
            Mesh3d(look.mesh.clone()),
            MeshMaterial3d(look.material.clone()),
            from,
        ));
    }
}

/// Torn-off Planks tumble to the ground, lie there a moment, then sink away.
fn fly_torn_planks(
    mut commands: Commands,
    mut planks: Query<(Entity, &mut TornPlank, &mut Transform)>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    for (entity, mut plank, mut transform) in &mut planks {
        plank.age += dt;
        if plank.age >= TORN_PLANK_SECS {
            commands.entity(entity).despawn();
            continue;
        }
        // Lying flat on the floor (at height 0 outside every window).
        let resting = 0.03;
        if transform.translation.y > resting || plank.velocity.y > 0.0 {
            plank.velocity.y -= GRAVITY * dt;
            transform.translation += plank.velocity * dt;
            let spin = plank.spin * dt;
            transform.rotate(Quat::from_euler(EulerRot::XYZ, spin.x, spin.y, spin.z));
        } else {
            transform.translation.y = resting;
            plank.velocity = Vec3::ZERO;
            let (yaw, _, _) = transform.rotation.to_euler(EulerRot::YXZ);
            let flat = Quat::from_rotation_y(yaw);
            transform.rotation = transform.rotation.slerp(flat, (12.0 * dt).min(1.0));
        }
        // Sinks into the floor over its last half second.
        let fade = ((TORN_PLANK_SECS - plank.age) / 0.5).min(1.0);
        transform.scale = Vec3::new(1.0, fade.max(0.05), 1.0);
    }
}

/// Zombies outside a window walk to an attack spot (or wait for one), tear Planks, swipe through
/// the window, and vault in once it's open.
fn enter_through_windows(
    mut commands: Commands,
    // A Zombie knocked down by the Thundergun picks up where it left off once it's back up.
    mut zombies: Query<
        (Entity, &Zombie, &mut Entering, &mut Transform),
        (Without<Dying>, Without<KnockedDown>, Without<Stunned>),
    >,
    players: Players,
    mut barriers: ResMut<Barriers>,
    mut hits: MessageWriter<PlayerHit>,
    mut torn: MessageWriter<PlankTorn>,
    mut damage: MessageWriter<DamageZombie>,
    mut cues: MessageWriter<PlayCue>,
    round: Res<Round>,
    time: Res<Time>,
) {
    let entries = players.first().and_then(|player| players.modifiers(player.entity)).map_or(&[][..], |modifiers| &modifiers.0[..]);
    let tear_damage = plank_tear_damage(round.number, entries);
    // The Player whose entries decide the tear's damage is the one it credits.
    let tearing_for = players.first().map(|player| player.entity);
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
                let victim = players
                    .iter()
                    .find(|player| player.up && reach_through(player.centre, transform.translation, centre));
                let in_reach = victim.is_some();
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
                        if let Some(victim) = victim
                            && rand::random_bool(BARRIER_SWIPE_CHANCE)
                        {
                            hits.write(PlayerHit {
                                player: victim.entity,
                                damage: ZOMBIE_SWIPE_DAMAGE,
                            });
                            cues.write(PlayCue {
                                cue: Cue::ZombieAttack,
                                at: Some(transform.translation),
                            });
                        }
                    }
                    SpotAction::Tear => {
                        barriers.tear(barrier);
                        torn.write(PlankTorn {
                            at: centre + Vec3::Y * 1.2,
                            barrier,
                            // The top one comes off first; see `show_planks`.
                            plank: barriers.planks(barrier),
                        });
                        if tear_damage > 0.0 {
                            damage.write(DamageZombie {
                                player: tearing_for,
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

/// Holding the Use key at a Barrier missing Planks asks to put them back one at a time. Its Prompt
/// comes from the shared pick, so it never shows while Down.
fn rebuild_planks(
    controls: Controls,
    focus: Res<Focus>,
    windows: Query<&Barrier>,
    barriers: Res<Barriers>,
    mut rebuild: ResMut<Rebuild>,
    local: Single<Entity, With<LocalPlayer>>,
    mut requests: MessageWriter<UseRequest>,
    time: Res<Time>,
) {
    let target = focus
        .0
        .filter(|_| controls.pressed(Action::Use))
        .and_then(|entity| windows.get(entity).ok().map(|window| (entity, window.0)))
        .filter(|&(_, barrier)| barriers.planks(barrier) < BARRIER_PLANKS);
    let advanced = rebuild.advance(target.map(|(_, barrier)| barrier), time.delta_secs());
    rebuild.animate(time.delta_secs());
    if let Some((buyable, _)) = target
        && advanced
    {
        requests.write(UseRequest { player: *local, buyable, hold: false });
    }
}

/// Puts a Plank back for each request at a Barrier still missing some, paying the Player who made
/// it up to the Round cap.
fn rebuild_barriers(
    mut requests: MessageReader<UseRequest>,
    windows: Query<&Barrier>,
    mut barriers: ResMut<Barriers>,
    mut rebuild: ResMut<Rebuild>,
    round: Res<Round>,
    mut players: Query<(&Modifiers, &mut Points), With<Player>>,
    mut awarded: MessageWriter<PointsAwarded>,
    mut rebuilt: MessageWriter<PlankRebuilt>,
) {
    for request in requests.read() {
        let Ok(window) = windows.get(request.buyable) else { continue };
        let barrier = window.0;
        if barriers.planks(barrier) >= BARRIER_PLANKS {
            continue;
        }
        let Ok((modifiers, mut points)) = players.get_mut(request.player) else { continue };
        barriers.rebuild(barrier);
        rebuilt.write(PlankRebuilt {
            at: window_centre(barrier) + Vec3::Y * 1.2,
        });
        let reward = rebuild.pay(round.number, PlankRates::resolve(round.number, &modifiers.0));
        if reward > 0 {
            let paid = points.award(reward);
            awarded.write(PointsAwarded { player: request.player, amount: paid });
        }
    }
}

/// Where a Plank is `t` of the way (0 to 1) through sliding into the `p`th slot: back inside the
/// room, tipped up, and swinging in to be flat against the window.
fn sliding_transform(p: u8, t: f32) -> Transform {
    let home = plank_transform(p);
    let t = t.clamp(0.0, 1.0);
    // Quick at first, settling into the slot.
    let left = (1.0 - t).powi(3);
    Transform {
        translation: home.translation + Vec3::Z * PLANK_SLIDE_FROM * left,
        rotation: home.rotation * Quat::from_rotation_x(0.9 * left),
        scale: Vec3::ONE,
    }
}

/// Planks just put back slide into their slots, then stay there.
fn slide_planks(
    mut commands: Commands,
    mut planks: Query<(Entity, &Plank, &mut Sliding, &mut Transform)>,
    time: Res<Time>,
) {
    for (entity, plank, mut sliding, mut transform) in &mut planks {
        sliding.0 += time.delta_secs() / PLANK_SLIDE_SECS;
        *transform = sliding_transform(plank.0, sliding.0);
        if sliding.0 >= 1.0 {
            commands.entity(entity).remove::<Sliding>();
        }
    }
}

/// Each Barrier shows the Planks it has left, and offers its Prompt only while missing some.
fn show_planks(
    mut commands: Commands,
    windows: Query<(Entity, &Barrier, Has<Buyable>, &Children)>,
    mut planks: Query<(Entity, &Plank, &mut Visibility)>,
    barriers: Res<Barriers>,
) {
    for (entity, window, has_prompt, children) in &windows {
        let left = barriers.planks(window.0);
        for child in children {
            if let Ok((plank_entity, plank, mut visibility)) = planks.get_mut(*child) {
                let shown = if plank.0 < left {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                // A Plank that wasn't there a moment ago slides in.
                if shown == Visibility::Inherited && *visibility == Visibility::Hidden {
                    commands.entity(plank_entity).insert(Sliding(0.0));
                }
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
                range: BARRIER_REBUILD_RANGE,
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
    fn a_sliding_plank_comes_in_from_inside_and_settles_in_its_slot() {
        let start = sliding_transform(2, 0.0);
        let home = plank_transform(2);
        assert!(start.translation.z > home.translation.z + 0.5, "back inside the room");
        let end = sliding_transform(2, 1.0);
        assert!(end.translation.distance(home.translation) < 1e-5);
        assert!(end.rotation.angle_between(home.rotation) < 1e-5);
        let mut last = start.translation;
        for i in 1..=100 {
            let at = sliding_transform(2, i as f32 / 100.0).translation;
            assert!(at.z <= last.z + 1e-6, "only ever moves toward the window");
            last = at;
        }
    }

    #[test]
    fn the_rebuild_hand_comes_up_with_a_hold_and_goes_with_it() {
        let mut rebuild = Rebuild::default();
        let window = Some(BarrierId(0));
        for _ in 0..100 {
            rebuild.advance(window, 0.01);
            rebuild.animate(0.01);
        }
        assert_eq!(rebuild.hand().0, 1.0);
        for _ in 0..100 {
            rebuild.advance(None, 0.01);
            rebuild.animate(0.01);
        }
        assert_eq!(rebuild.hand(), (0.0, 0.0));
    }

    #[test]
    fn the_rebuild_hand_pushes_once_per_plank() {
        let mut rebuild = Rebuild::default();
        let window = Some(BarrierId(0));
        let mut peaks = 0;
        let mut last = 0.0;
        let mut rising = false;
        for _ in 0..300 {
            rebuild.advance(window, 0.01);
            rebuild.animate(0.01);
            let reach = rebuild.hand().1;
            if reach > last {
                rising = true;
            } else if reach < last && rising {
                peaks += 1;
                rising = false;
            }
            last = reach;
        }
        assert_eq!(peaks, 3, "three Planks in three seconds");
    }

    #[test]
    fn every_barrier_starts_with_six_planks() {
        let barriers = Barriers::default();
        for i in 0..WINDOWS.len() {
            assert_eq!(barriers.planks(BarrierId(i as u8)), 6);
            assert!(!barriers.is_open(BarrierId(i as u8)));
        }
    }

    #[test]
    fn there_are_twenty_barriers_and_none_in_the_yard() {
        assert_eq!(WINDOWS.len(), 20);
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
    fn the_sapper_smashes_all_six_planks_at_once() {
        let mut barriers = Barriers::default();
        assert_eq!(barriers.smash(BarrierId(2)), 6);
        assert!(barriers.is_open(BarrierId(2)));
        assert_eq!(barriers.smash(BarrierId(2)), 0);
        assert_eq!(barriers.planks(BarrierId(3)), 6);
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
