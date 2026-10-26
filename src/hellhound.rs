//! Hellhounds: the Special Zombie of Hellhound Rounds, which replace the normal Horde every few
//! Rounds. A hound arrives in a lightning strike, runs fast, bites, and never uses Barriers. The
//! fog pulls in for the Round. From Round 16 a few hounds mix into normal Rounds too.
//!
//! The Round decides when hounds spawn (see `round.rs`); this module holds their rules, their
//! bodies, the lightning, the fog, and `SpawnHellhounds` for anything that calls hounds in.

use std::f32::consts::{FRAC_PI_2, TAU};

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::layers::GameLayer;
use crate::nav::Route;
use crate::player::PlayerCamera;
use crate::round::{Round, RoundPhase};
use crate::state::{InRun, Pause};
use crate::tuning::*;
use crate::zombie::{HitArea, HitZone, Zombie};
use crate::zone::{ZoneId, ZoneSet};

/// When the Hellhound Rounds fall: the first in `HELLHOUND_FIRST_ROUNDS`, then one every
/// `HELLHOUND_ROUND_GAPS` Rounds after the last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HellhoundSchedule {
    next: u32,
    played: u32,
}

impl HellhoundSchedule {
    /// A Run's schedule, its first Hellhound Round drawn from `HELLHOUND_FIRST_ROUNDS`.
    pub fn new(first: u32) -> Self {
        Self {
            next: first,
            played: 0,
        }
    }

    /// Round `round` begins. Returns which Hellhound Round it is (1 for the first), if it is one,
    /// and books the next one `gap` Rounds later.
    pub fn begin(&mut self, round: u32, gap: u32) -> Option<u32> {
        if round < self.next {
            return None;
        }
        self.played += 1;
        self.next = round.saturating_add(gap);
        Some(self.played)
    }

    /// How many Hellhound Rounds have begun so far.
    pub fn played(&self) -> u32 {
        self.played
    }
}

/// The Horde of Hellhound Round `count` (1 for the first).
pub fn hellhound_horde(count: u32) -> u32 {
    if count <= HELLHOUND_HORDE_EARLY_ROUNDS {
        HELLHOUND_HORDE_EARLY
    } else {
        HELLHOUND_HORDE
    }
}

/// Seconds between hounds in Hellhound Round `count`.
pub fn hellhound_spawn_gap(count: u32) -> f32 {
    by_count(&HELLHOUND_SPAWN_GAPS_SECS, count)
}

/// A hound's health in Hellhound Round `count`. Mixed and called-in hounds use the latest
/// Hellhound Round's.
pub fn hellhound_health(count: u32) -> f32 {
    by_count(&HELLHOUND_HEALTH, count)
}

/// The entry for Hellhound Round `count` in a table whose last entry covers every later one.
fn by_count(table: &[f32], count: u32) -> f32 {
    let i = (count.max(1) as usize - 1).min(table.len() - 1);
    table[i]
}

/// The chance a normal spawn in Round `round` is a hound instead: none before Round 16.
pub fn mixed_hound_chance(round: u32) -> f64 {
    HELLHOUND_MIXED_CHANCE
        .iter()
        .rev()
        .find(|(from, _)| round >= *from)
        .map_or(0.0, |(_, chance)| *chance)
}

/// The Hellhound Spawn Points a hound picks from, uniformly: those in Active Zones between
/// `HELLHOUND_SPAWN_MIN_DIST` and `HELLHOUND_SPAWN_MAX_DIST` of the Player. If none is, the one
/// nearest that band, so a Hellhound Round never stalls for want of a spot.
pub fn hellhound_spawn_candidates(
    points: impl IntoIterator<Item = (ZoneId, Vec3)>,
    active: ZoneSet,
    player: Vec3,
) -> Vec<Vec3> {
    let active: Vec<Vec3> = points
        .into_iter()
        .filter(|(zone, _)| active.contains(*zone))
        .map(|(_, at)| at)
        .collect();
    let off_band = |at: &Vec3| {
        let d = at.distance(player);
        (HELLHOUND_SPAWN_MIN_DIST - d)
            .max(d - HELLHOUND_SPAWN_MAX_DIST)
            .max(0.0)
    };
    let in_band: Vec<Vec3> = active
        .iter()
        .copied()
        .filter(|at| off_band(at) == 0.0)
        .collect();
    if !in_band.is_empty() {
        return in_band;
    }
    active
        .into_iter()
        .min_by(|a, b| off_band(a).total_cmp(&off_band(b)))
        .into_iter()
        .collect()
}

/// The fog's linear falloff (start, end) to head for: pulled in while a Hellhound Round is under
/// way, normal otherwise.
pub fn fog_target(hellhound_round: bool) -> (f32, f32) {
    if hellhound_round {
        (HELLHOUND_FOG_START, HELLHOUND_FOG_END)
    } else {
        (FOG_START, FOG_END)
    }
}

/// `current` moved toward `target` by at most `step`.
fn approach(current: f32, target: f32, step: f32) -> f32 {
    current + (target - current).clamp(-step, step)
}

/// A hound in its lightning strike: it can't be hurt, move or bite until it's over.
#[derive(Component)]
pub struct Telegraph {
    left: f32,
    /// The bolt of lightning, gone when the strike ends.
    bolt: Entity,
}

/// Calls in `count` hounds around `around` (a point on the floor), joining the current Round at
/// the latest Hellhound Round's health. The Teleporter uses this.
#[derive(Message, Debug, Clone, Copy)]
pub struct SpawnHellhounds {
    pub around: Vec3,
    pub count: u32,
}

#[derive(Resource)]
struct HellhoundAssets {
    body: Handle<Mesh>,
    head: Handle<Mesh>,
    leg: Handle<Mesh>,
    eye: Handle<Mesh>,
    bolt: Handle<Mesh>,
    hide: Handle<StandardMaterial>,
    eyes: Handle<StandardMaterial>,
    lightning: Handle<StandardMaterial>,
}

/// Lets the Round spawn hounds without knowing about the asset handles.
#[derive(bevy::ecs::system::SystemParam)]
pub struct HellhoundAssetsParam<'w>(Res<'w, HellhoundAssets>);

const LEG_SIZE: Vec3 = Vec3::new(0.1, 0.35, 0.1);
const NECK_SIZE: Vec3 = Vec3::new(0.18, 0.18, 0.15);
const HEAD_RADIUS: f32 = 0.15;
const BODY_RADIUS: f32 = 0.2;
const BODY_LENGTH: f32 = 0.6;
const BOLT_HEIGHT: f32 = 16.0;

pub struct HellhoundPlugin;

impl Plugin for HellhoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SpawnHellhounds>()
            .add_systems(Startup, load_assets)
            .add_systems(
                Update,
                (spawn_around, strike, pull_in_fog).run_if(in_state(Pause::Running)),
            );
    }
}

fn load_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(HellhoundAssets {
        // Lying along its facing (-Z).
        body: meshes.add(
            Mesh::from(Capsule3d::new(BODY_RADIUS, BODY_LENGTH))
                .rotated_by(Quat::from_rotation_x(FRAC_PI_2)),
        ),
        head: meshes.add(Sphere::new(HEAD_RADIUS)),
        leg: meshes.add(Cuboid::from_size(LEG_SIZE)),
        eye: meshes.add(Sphere::new(0.03)),
        bolt: meshes.add(Cylinder::new(0.08, BOLT_HEIGHT)),
        hide: materials.add(StandardMaterial {
            base_color: Color::srgb(0.12, 0.08, 0.07),
            emissive: LinearRgba::rgb(0.25, 0.04, 0.0),
            ..default()
        }),
        eyes: materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.3, 0.05),
            emissive: LinearRgba::rgb(10.0, 2.0, 0.2),
            ..default()
        }),
        lightning: materials.add(StandardMaterial {
            base_color: Color::srgb(0.8, 0.85, 1.0),
            emissive: LinearRgba::rgb(20.0, 22.0, 30.0),
            unlit: true,
            ..default()
        }),
    });
}

/// Spawns a hound standing on the floor at `at`, inside its lightning strike. Hounds never get
/// `Entering`: they come in at Hellhound Spawn Points inside the Zones, so never use Barriers.
pub fn spawn_hellhound(
    commands: &mut Commands,
    assets: &HellhoundAssetsParam,
    at: Vec3,
    health: f32,
) -> Entity {
    let assets = &assets.0;
    let centre = at + Vec3::Y * (HELLHOUND_RADIUS + 0.02);
    let bolt = commands
        .spawn((
            InRun,
            Mesh3d(assets.bolt.clone()),
            MeshMaterial3d(assets.lightning.clone()),
            Transform::from_translation(at + Vec3::Y * BOLT_HEIGHT / 2.0),
            children![(
                PointLight {
                    color: Color::srgb(0.7, 0.8, 1.0),
                    intensity: 400_000.0,
                    range: 12.0,
                    ..default()
                },
                Transform::from_xyz(0.0, 1.5 - BOLT_HEIGHT / 2.0, 0.0),
            )],
        ))
        .id();
    commands
        .spawn((
            InRun,
            Name::new("Hellhound"),
            Zombie::hellhound(health, centre),
            Telegraph {
                left: HELLHOUND_TELEGRAPH_SECS,
                bolt,
            },
            Route::default(),
            Mesh3d(assets.body.clone()),
            MeshMaterial3d(assets.hide.clone()),
            Transform::from_translation(centre),
            RigidBody::Kinematic,
            CustomPositionIntegration,
            TranslationInterpolation,
            Collider::sphere(HELLHOUND_RADIUS),
            CollisionLayers::new(GameLayer::Zombie, LayerMask::ALL),
        ))
        .with_children(|hound| {
            let hit_layers = CollisionLayers::new(GameLayer::ZombieHitArea, LayerMask::ALL);
            let body_y = 0.1;
            let front = -(BODY_LENGTH / 2.0 + BODY_RADIUS);
            hound.spawn((
                HitArea(HitZone::Torso),
                Transform::from_xyz(0.0, body_y, 0.0),
                Collider::cuboid(
                    BODY_RADIUS * 2.0,
                    BODY_RADIUS * 2.0,
                    BODY_LENGTH + BODY_RADIUS * 2.0,
                ),
                hit_layers,
            ));
            hound.spawn((
                HitArea(HitZone::Neck),
                Transform::from_xyz(0.0, body_y + 0.15, front - NECK_SIZE.z / 2.0),
                Collider::cuboid(NECK_SIZE.x, NECK_SIZE.y, NECK_SIZE.z),
                hit_layers,
            ));
            hound
                .spawn((
                    HitArea(HitZone::Head),
                    Mesh3d(assets.head.clone()),
                    MeshMaterial3d(assets.hide.clone()),
                    Transform::from_xyz(0.0, body_y + 0.2, front - NECK_SIZE.z - HEAD_RADIUS * 0.8),
                    Collider::sphere(HEAD_RADIUS),
                    hit_layers,
                ))
                .with_children(|head| {
                    for x in [-0.06, 0.06] {
                        head.spawn((
                            Mesh3d(assets.eye.clone()),
                            MeshMaterial3d(assets.eyes.clone()),
                            Transform::from_xyz(x, 0.04, -HEAD_RADIUS * 0.9),
                        ));
                    }
                });
            // Four legs down to the floor.
            let leg_y = -HELLHOUND_RADIUS + LEG_SIZE.y / 2.0;
            for (x, z) in [(-0.13, -0.3), (0.13, -0.3), (-0.13, 0.3), (0.13, 0.3)] {
                hound.spawn((
                    HitArea(HitZone::Limb),
                    Mesh3d(assets.leg.clone()),
                    MeshMaterial3d(assets.hide.clone()),
                    Transform::from_xyz(x, leg_y, z),
                    Collider::cuboid(LEG_SIZE.x, LEG_SIZE.y, LEG_SIZE.z),
                    hit_layers,
                ));
            }
        })
        .id()
}

/// Hounds called in around a point land in a ring around it, each in its own strike.
fn spawn_around(
    mut commands: Commands,
    mut requests: MessageReader<SpawnHellhounds>,
    round: Option<Res<Round>>,
    assets: HellhoundAssetsParam,
) {
    let Some(round) = round else {
        requests.clear();
        return;
    };
    let health = hellhound_health(round.hellhound_tier());
    for request in requests.read() {
        for i in 0..request.count {
            // Spread evenly round the point, with a little jitter so they don't line up.
            let angle = TAU * (i as f32 + rand::random_range(0.0..0.5)) / request.count as f32;
            let dist = rand::random_range(HELLHOUND_AROUND_MIN_DIST..HELLHOUND_AROUND_MAX_DIST);
            let at = request.around + Vec3::new(angle.cos(), 0.0, angle.sin()) * dist;
            spawn_hellhound(&mut commands, &assets, at, health);
        }
    }
}

/// A strike flickers for `HELLHOUND_TELEGRAPH_SECS`, then the hound is loose.
fn strike(
    mut commands: Commands,
    mut hounds: Query<(Entity, &mut Telegraph)>,
    mut bolts: Query<&mut Visibility>,
    time: Res<Time>,
) {
    for (entity, mut telegraph) in &mut hounds {
        telegraph.left -= time.delta_secs();
        if telegraph.left <= 0.0 {
            commands.entity(telegraph.bolt).despawn();
            commands.entity(entity).remove::<Telegraph>();
        } else if let Ok(mut visibility) = bolts.get_mut(telegraph.bolt) {
            let shown = rand::random_bool(0.6);
            visibility.set_if_neq(if shown {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
    }
}

/// The fog pulls in while a Hellhound Round is under way, and clears again in the Intermission.
fn pull_in_fog(
    round: Option<Res<Round>>,
    mut fogs: Query<&mut DistanceFog, With<PlayerCamera>>,
    time: Res<Time>,
) {
    let hellhound_round =
        round.is_some_and(|round| round.hellhound.is_some() && round.phase == RoundPhase::Active);
    let (start, end) = fog_target(hellhound_round);
    let step = FOG_PULL_SPEED * time.delta_secs();
    for mut fog in &mut fogs {
        if let FogFalloff::Linear { start: s, end: e } = fog.falloff
            && (s != start || e != end)
        {
            fog.falloff = FogFalloff::Linear {
                start: approach(s, start, step),
                end: approach(e, end, step),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Hellhound Rounds a schedule gives through Round 60, with every draw at `first` and
    /// `gap`.
    fn rounds(first: u32, gap: u32) -> Vec<u32> {
        let mut schedule = HellhoundSchedule::new(first);
        (1..=60)
            .filter(|&r| schedule.begin(r, gap).is_some())
            .collect()
    }

    #[test]
    fn the_first_hellhound_round_is_round_five_to_seven() {
        for first in HELLHOUND_FIRST_ROUNDS {
            assert_eq!(rounds(first, 4)[0], first);
        }
        assert_eq!(HELLHOUND_FIRST_ROUNDS, 5..=7);
    }

    #[test]
    fn then_one_every_four_to_five_rounds() {
        assert_eq!(rounds(5, 4)[..4], [5, 9, 13, 17]);
        assert_eq!(rounds(7, 5)[..4], [7, 12, 17, 22]);
        assert_eq!(HELLHOUND_ROUND_GAPS, 4..=5);
    }

    #[test]
    fn hellhound_rounds_count_up() {
        let mut schedule = HellhoundSchedule::new(6);
        assert_eq!(schedule.begin(5, 4), None);
        assert_eq!(schedule.begin(6, 4), Some(1));
        assert_eq!(schedule.begin(7, 4), None);
        assert_eq!(schedule.begin(10, 5), Some(2));
        assert_eq!(schedule.played(), 2);
        assert_eq!(schedule.begin(14, 4), None);
        assert_eq!(schedule.begin(15, 4), Some(3));
    }

    #[test]
    fn hordes_are_six_then_eight() {
        assert_eq!([1, 2, 3, 4, 10].map(hellhound_horde), [6, 6, 8, 8, 8]);
    }

    #[test]
    fn spawn_gaps_shrink_to_a_second_and_a_half() {
        assert_eq!(
            [1, 2, 3, 4, 9].map(hellhound_spawn_gap),
            [3.0, 2.5, 2.0, 1.5, 1.5]
        );
    }

    #[test]
    fn health_tiers_follow_the_hellhound_round_count() {
        assert_eq!(
            [1, 2, 3, 4, 5, 20].map(hellhound_health),
            [400.0, 900.0, 1300.0, 1600.0, 1600.0, 1600.0]
        );
        assert_eq!(
            hellhound_health(0),
            400.0,
            "before any Hellhound Round, the first tier"
        );
    }

    #[test]
    fn mixed_hounds_start_at_round_sixteen() {
        let table = [
            (1, 0.0),
            (15, 0.0),
            (16, 0.01),
            (20, 0.01),
            (21, 0.02),
            (30, 0.02),
            (31, 0.03),
            (100, 0.03),
        ];
        for (round, chance) in table {
            assert_eq!(mixed_hound_chance(round), chance, "round {round}");
        }
    }

    #[test]
    fn hounds_spawn_in_active_zones_ten_to_twenty_five_metres_out() {
        use ZoneId::*;
        let player = Vec3::ZERO;
        let points = [
            (StartRoom, Vec3::new(5.0, 0.0, 0.0)),
            (StartRoom, Vec3::new(12.0, 0.0, 0.0)),
            (StartRoom, Vec3::new(0.0, 0.0, -24.0)),
            (Yard, Vec3::new(30.0, 0.0, 0.0)),
            (WestHall, Vec3::new(15.0, 0.0, 0.0)),
        ];
        let active = ZoneSet::from_iter([StartRoom, Yard]);
        assert_eq!(
            hellhound_spawn_candidates(points, active, player),
            [Vec3::new(12.0, 0.0, 0.0), Vec3::new(0.0, 0.0, -24.0)]
        );
    }

    #[test]
    fn with_no_point_in_range_the_nearest_to_it_is_used() {
        use ZoneId::*;
        let points = [
            (StartRoom, Vec3::new(3.0, 0.0, 0.0)),
            (StartRoom, Vec3::new(8.0, 0.0, 0.0)),
            (Yard, Vec3::new(12.0, 0.0, 0.0)),
        ];
        let active = ZoneSet::from_iter([StartRoom]);
        assert_eq!(
            hellhound_spawn_candidates(points, active, Vec3::ZERO),
            [Vec3::new(8.0, 0.0, 0.0)]
        );
        assert!(hellhound_spawn_candidates(points, ZoneSet::default(), Vec3::ZERO).is_empty());
    }

    #[test]
    fn the_fog_pulls_in_for_a_hellhound_round() {
        let (start, end) = fog_target(true);
        assert!(start < FOG_START && end < FOG_END);
        assert_eq!(fog_target(false), (FOG_START, FOG_END));
        assert_eq!(approach(45.0, 20.0, 5.0), 40.0);
        assert_eq!(approach(21.0, 20.0, 5.0), 20.0);
        assert_eq!(approach(10.0, 20.0, 5.0), 15.0);
    }
}
