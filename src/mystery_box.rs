//! The Mystery Box: spin it for a random Box Weapon, then take it before it sinks back in. A Teddy
//! Bear refunds the spin and sends the Box to another Box Location.
//!
//! Each Box is its own entity with its own price and spin, so several can stand at once. The Box
//! that started the Run is the one that rolls Teddies and moves; a Box spawned with `teddy` off
//! (as Fire Sale's will be) never does, and its spins don't count toward the Teddy table.

use avian3d::prelude::*;
use bevy::prelude::*;
use rand::seq::SliceRandom;
use rand::{Rng, RngExt};

use crate::arsenal::{Availability, WeaponClass, WeaponId};
use crate::buy::{Buyable, Purchased};
use crate::layers::GameLayer;
use crate::map::BOX_LOCATIONS;
use crate::modifier::Modifiers;
use crate::player::Player;
use crate::points::Points;
use crate::state::{GameState, InRun, Pause};
use crate::tuning::*;
use crate::weapon::Loadout;

/// The Box's size: long side, height, depth.
const BOX_SIZE: Vec3 = Vec3::new(1.6, 0.8, 0.9);
const BEAM_HEIGHT: f32 = 60.0;
const BEAM_RADIUS: f32 = 0.3;
/// How fast the Teddy Bear flies up, in metres a second.
const TEDDY_FLY_SPEED: f32 = 3.0;
/// Where a spun Weapon floats over the Box's centre: it rises to the top while spinning, then
/// sinks back toward the lid until it's taken or lost.
const PRIZE_LOW: f32 = 0.45;
const PRIZE_HIGH: f32 = 1.0;

/// What the Box can give. Equipment such as the Monkey Bomb joins as its own kind of prize.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prize {
    Weapon(WeaponId),
}

impl Prize {
    pub fn name(self) -> &'static str {
        match self {
            Prize::Weapon(id) => id.stats().name,
        }
    }
}

/// How a spin ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spin {
    Prize(Prize),
    Teddy,
}

/// Where a Box is in its cycle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BoxState {
    /// Ready to spin.
    Idle,
    /// Cycling through the pool. `paid` is refunded on a Teddy; `teddy` is whether this spin can
    /// give one (and counts toward the Teddy table).
    Spinning { elapsed: f32, paid: u64, teddy: bool },
    /// A Prize waits to be taken.
    Offering { prize: Prize, left: f32, counts: bool },
    /// Waiting to be spun again.
    Rearming { left: f32 },
    /// The Teddy Bear rises and flies off, then the Box shakes.
    Leaving { elapsed: f32 },
    /// Gone, reappearing at Box Location `to`.
    Away { left: f32, to: usize },
}

/// A Mystery Box standing at one of the Box Locations.
#[derive(Component)]
pub struct MysteryBox {
    /// Index into `BOX_LOCATIONS`.
    pub location: usize,
    /// What a spin costs. Whoever owns the Box's price (Fire Sale, a discount) rewrites it.
    pub price: u64,
    /// Whether this Box's spins can give a Teddy Bear and count toward the Teddy table.
    pub teddy: bool,
    pub state: BoxState,
}

impl MysteryBox {
    /// Whether the Box is standing anywhere, rather than between locations.
    pub fn present(&self) -> bool {
        !matches!(self.state, BoxState::Away { .. })
    }
}

/// A Box Location, showing a rubble marker while no Box stands there.
#[derive(Component)]
pub struct BoxLocation(pub usize);

/// The moving Box's history this Run.
#[derive(Resource, Default)]
pub struct BoxRun {
    /// Box Locations still to visit before the list reshuffles, next first.
    bag: Vec<usize>,
    /// Completed pulls (taken or lost) at the current location.
    pub pulls: u32,
    /// How many times the Box has moved this Run.
    pub moves: u32,
}

/// The parts of a Box that animate.
#[derive(Component)]
struct BoxParts {
    body: Entity,
    prize: Entity,
    teddy: Entity,
}

/// Every Box's meshes and materials, made once, so any owner can spawn another Box.
#[derive(Resource)]
pub struct BoxAssets {
    body: Handle<Mesh>,
    body_mat: Handle<StandardMaterial>,
    beam: Handle<Mesh>,
    beam_mat: Handle<StandardMaterial>,
    prize: Handle<Mesh>,
    /// The spinning Weapon's looks, cycled through each step.
    prize_mats: Vec<Handle<StandardMaterial>>,
    teddy: Handle<Mesh>,
    teddy_mat: Handle<StandardMaterial>,
    rubble: Handle<Mesh>,
    rubble_mat: Handle<StandardMaterial>,
}

impl FromWorld for BoxAssets {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        let body = meshes.add(Cuboid::from_size(BOX_SIZE));
        let beam = meshes.add(Cylinder::new(BEAM_RADIUS, BEAM_HEIGHT));
        let prize = meshes.add(Cuboid::new(0.9, 0.16, 0.08));
        let teddy = meshes.add(Cuboid::new(0.3, 0.45, 0.22));
        let rubble = meshes.add(Cuboid::new(0.5, 0.25, 0.4));
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let prize_mats = [
            Color::srgb(0.8, 0.8, 0.85),
            Color::srgb(0.25, 0.25, 0.28),
            Color::srgb(0.55, 0.4, 0.2),
            Color::srgb(0.3, 0.45, 0.25),
            Color::srgb(0.7, 0.6, 0.3),
            Color::srgb(0.4, 0.3, 0.5),
        ]
        .map(|color| materials.add(color))
        .to_vec();
        Self {
            body,
            body_mat: materials.add(StandardMaterial {
                base_color: Color::srgb(0.2, 0.25, 0.5),
                emissive: LinearRgba::rgb(0.05, 0.1, 0.4),
                ..default()
            }),
            beam,
            beam_mat: materials.add(StandardMaterial {
                base_color: Color::srgba(0.55, 0.8, 1.0, 0.22),
                emissive: LinearRgba::rgb(0.6, 0.9, 1.5),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                ..default()
            }),
            prize,
            prize_mats,
            teddy,
            teddy_mat: materials.add(Color::srgb(0.55, 0.35, 0.18)),
            rubble,
            rubble_mat: materials.add(Color::srgb(0.3, 0.29, 0.27)),
        }
    }
}

/// The Teddy Bear's chance on a spin, from the completed pulls at this location and whether the
/// Box is still at the location it started the Run at.
pub fn teddy_chance(pulls: u32, first_location: bool) -> f64 {
    if pulls < BOX_TEDDY_FREE_PULLS {
        0.0
    } else if pulls < BOX_TEDDY_LATE_PULLS {
        BOX_TEDDY_CHANCE_EARLY
    } else if first_location {
        BOX_TEDDY_CHANCE_FIRST_LOCATION
    } else if pulls < BOX_TEDDY_LATEST_PULLS {
        BOX_TEDDY_CHANCE_LATE
    } else {
        BOX_TEDDY_CHANCE_LATEST
    }
}

/// Whether the Box serves this Weapon yet. The special Box Weapons (Ray Gun, Thundergun,
/// Crossbow, Ballistic Knife) join once they fire like their BO1 selves.
fn served(id: WeaponId) -> bool {
    !matches!(
        id.stats().class,
        WeaponClass::RayGun | WeaponClass::Thundergun | WeaponClass::Crossbow | WeaponClass::BallisticKnife
    )
}

/// What a spin can give, each at weight 1. It skips any Weapon the Player holds (an Upgraded copy
/// is still its base Weapon) and any limited Weapon already in play elsewhere, like in another Box.
pub fn box_pool(held: &[WeaponId], in_play: &[WeaponId]) -> Vec<Prize> {
    crate::tuning::WEAPONS
        .iter()
        .filter(|row| match row.availability {
            Availability::Box => true,
            Availability::LimitedBox => !in_play.contains(&row.id),
            Availability::Start | Availability::Wall(_) => false,
        })
        .map(|row| row.id)
        .filter(|&id| served(id) && !held.contains(&id))
        .map(Prize::Weapon)
        .collect()
}

/// How a spin ends: a Teddy at `teddy_chance`, otherwise a Prize from the pool. An empty pool
/// gives a Teddy, so the spin is refunded.
pub fn roll_spin<R: Rng + ?Sized>(teddy_chance: f64, pool: &[Prize], rng: &mut R) -> Spin {
    if rng.random::<f64>() < teddy_chance {
        return Spin::Teddy;
    }
    match pool {
        [] => Spin::Teddy,
        pool => Spin::Prize(pool[rng.random_range(0..pool.len())]),
    }
}

/// The Box Locations in a fresh random order, never putting `current` first.
pub fn shuffled_locations<R: Rng + ?Sized>(count: usize, current: usize, rng: &mut R) -> Vec<usize> {
    let mut bag: Vec<usize> = (0..count).collect();
    bag.shuffle(rng);
    if count > 1 && bag[0] == current {
        let other = rng.random_range(1..count);
        bag.swap(0, other);
    }
    bag
}

/// Where the Box goes after a Teddy: the next location in the Run's list, reshuffling it when
/// it's empty.
pub fn next_location<R: Rng + ?Sized>(bag: &mut Vec<usize>, count: usize, current: usize, rng: &mut R) -> usize {
    if bag.is_empty() {
        *bag = shuffled_locations(count, current, rng);
    }
    bag.remove(0)
}

/// Which of the spin's slowing steps is showing after `elapsed` seconds. Each step lasts longer
/// than the one before.
pub fn spin_step(elapsed: f32) -> u32 {
    let t = (elapsed / BOX_SPIN_SECS).clamp(0.0, 1.0);
    ((BOX_SPIN_STEPS as f32 * t.sqrt()) as u32).min(BOX_SPIN_STEPS)
}

/// How long the whole Teddy sequence takes, from the bear showing to the Box vanishing.
const LEAVING_SECS: f32 = BOX_TEDDY_RISE_SECS + BOX_TEDDY_FLY_SECS + BOX_SHAKE_SECS;

/// Where a Box at a Box Location sits: its centre, turned so its front faces the Player's side.
fn location_transform(location: usize) -> Transform {
    let (_, at, facing) = BOX_LOCATIONS[location];
    Transform::from_xyz(at.x, BOX_SIZE.y / 2.0, at.y).with_rotation(Quat::from_rotation_y(facing.x.atan2(facing.y)))
}

/// Spawns a Box at a Box Location. The Run's own Box is spawned here too; Fire Sale can add more.
pub fn spawn_box(commands: &mut Commands, assets: &BoxAssets, location: usize, price: u64, teddy: bool) -> Entity {
    let body = commands
        .spawn((Mesh3d(assets.body.clone()), MeshMaterial3d(assets.body_mat.clone()), Transform::default()))
        .id();
    let prize = commands
        .spawn((
            Mesh3d(assets.prize.clone()),
            MeshMaterial3d(assets.prize_mats[0].clone()),
            Transform::from_xyz(0.0, PRIZE_LOW, 0.0),
            Visibility::Hidden,
        ))
        .id();
    let teddy_bear = commands
        .spawn((
            Mesh3d(assets.teddy.clone()),
            MeshMaterial3d(assets.teddy_mat.clone()),
            Transform::from_xyz(0.0, PRIZE_LOW, 0.0),
            Visibility::Hidden,
        ))
        .id();
    // The always-on beam, standing on the Box, and a glow to find it up close.
    let beam = commands
        .spawn((
            Mesh3d(assets.beam.clone()),
            MeshMaterial3d(assets.beam_mat.clone()),
            Transform::from_xyz(0.0, BEAM_HEIGHT / 2.0, 0.0),
        ))
        .id();
    let glow = commands
        .spawn((
            PointLight {
                color: Color::srgb(0.5, 0.75, 1.0),
                intensity: 60_000.0,
                range: 6.0,
                ..default()
            },
            Transform::from_xyz(0.0, 1.2, 0.0),
        ))
        .id();
    let shape = Cuboid::from_size(BOX_SIZE);
    commands
        .spawn((
            InRun,
            MysteryBox {
                location,
                price,
                teddy,
                state: BoxState::Idle,
            },
            BoxParts {
                body,
                prize,
                teddy: teddy_bear,
            },
            location_transform(location),
            Visibility::default(),
            RigidBody::Static,
            Collider::from(shape),
            CollisionLayers::new(GameLayer::World, LayerMask::ALL),
        ))
        .add_children(&[body, prize, teddy_bear, beam, glow])
        .id()
}

pub struct MysteryBoxPlugin;

impl Plugin for MysteryBoxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BoxAssets>()
            .init_resource::<BoxRun>()
            .add_systems(OnEnter(GameState::Playing), spawn_boxes)
            .add_systems(
                Update,
                (buy_boxes, run_boxes, price_boxes, animate_boxes, show_rubble)
                    .chain()
                    .run_if(in_state(Pause::Running)),
            );
    }
}

/// Every Run starts with the Box at the first Box Location and rubble at the rest.
fn spawn_boxes(mut commands: Commands, assets: Res<BoxAssets>, mut run: ResMut<BoxRun>) {
    *run = BoxRun {
        bag: shuffled_locations(BOX_LOCATIONS.len(), 0, &mut rand::rng()),
        pulls: 0,
        moves: 0,
    };
    for location in 0..BOX_LOCATIONS.len() {
        let rubble = [(-0.4, 0.0, 0.3), (0.35, 0.1, -0.6), (0.1, -0.2, 1.2)].map(|(x, z, yaw)| {
            commands
                .spawn((
                    Mesh3d(assets.rubble.clone()),
                    MeshMaterial3d(assets.rubble_mat.clone()),
                    Transform::from_xyz(x, -BOX_SIZE.y / 2.0 + 0.125, z).with_rotation(Quat::from_rotation_y(yaw)),
                ))
                .id()
        });
        commands
            .spawn((InRun, BoxLocation(location), location_transform(location), Visibility::Hidden))
            .add_children(&rubble);
    }
    spawn_box(&mut commands, &assets, 0, BOX_PRICE, true);
}

/// A bought Box starts spinning; a bought Prize goes to the Player.
fn buy_boxes(
    mut purchases: MessageReader<Purchased>,
    mut boxes: Query<&mut MysteryBox>,
    player: Single<(&mut Loadout, &Modifiers), With<Player>>,
    mut run: ResMut<BoxRun>,
) {
    let (mut loadout, modifiers) = player.into_inner();
    for Purchased(entity) in purchases.read() {
        let Ok(mut mystery_box) = boxes.get_mut(*entity) else { continue };
        match mystery_box.state {
            BoxState::Idle => {
                mystery_box.state = BoxState::Spinning {
                    elapsed: 0.0,
                    paid: mystery_box.price,
                    teddy: mystery_box.teddy,
                };
            }
            BoxState::Offering { prize, counts, .. } => {
                match prize {
                    Prize::Weapon(id) => match loadout.slot_of(id) {
                        Some(slot) => loadout.refill(slot, &modifiers.0),
                        // Into the empty slot, or in place of the Weapon in hand.
                        None => loadout.give(id),
                    },
                }
                if counts {
                    run.pulls = run.pulls.saturating_add(1);
                }
                mystery_box.state = BoxState::Rearming { left: BOX_REARM_SECS };
            }
            _ => {}
        }
    }
}

/// Runs each Box's clocks: the spin's result, a Prize timing out, re-arming, and the Teddy's move.
fn run_boxes(
    mut commands: Commands,
    time: Res<Time>,
    mut boxes: Query<(Entity, &mut MysteryBox, &mut Transform, &mut Visibility)>,
    loadout: Single<&Loadout, With<Player>>,
    mut points: ResMut<Points>,
    mut run: ResMut<BoxRun>,
) {
    let dt = time.delta_secs();
    let held: Vec<WeaponId> = (0..2).filter_map(|slot| loadout.slot(slot)).map(|weapon| weapon.id).collect();
    // A Weapon waiting in a Box is in play.
    let mut in_play: Vec<WeaponId> = boxes
        .iter()
        .filter_map(|(_, mystery_box, ..)| match mystery_box.state {
            BoxState::Offering { prize: Prize::Weapon(id), .. } => Some(id),
            _ => None,
        })
        .collect();
    let rng = &mut rand::rng();
    for (entity, mut mystery_box, mut transform, mut visibility) in &mut boxes {
        mystery_box.state = match mystery_box.state {
            BoxState::Idle => continue,
            BoxState::Spinning { elapsed, paid, teddy } => {
                let elapsed = elapsed + dt;
                if elapsed < BOX_SPIN_SECS {
                    BoxState::Spinning { elapsed, paid, teddy }
                } else {
                    let chance = if teddy { teddy_chance(run.pulls, run.moves == 0) } else { 0.0 };
                    match roll_spin(chance, &box_pool(&held, &in_play), rng) {
                        Spin::Prize(prize) => {
                            let Prize::Weapon(id) = prize;
                            in_play.push(id);
                            BoxState::Offering {
                                prize,
                                left: BOX_PICKUP_SECS,
                                counts: teddy,
                            }
                        }
                        Spin::Teddy => {
                            points.refund(paid);
                            if teddy {
                                run.pulls = 0;
                                run.moves = run.moves.saturating_add(1);
                                BoxState::Leaving { elapsed: 0.0 }
                            } else {
                                // A Box that can't move only gets here with nothing left to give.
                                BoxState::Idle
                            }
                        }
                    }
                }
            }
            BoxState::Offering { prize, left, counts } => {
                let left = left - dt;
                if left > 0.0 {
                    BoxState::Offering { prize, left, counts }
                } else {
                    // Lost, with no refund. It still finishes the pull.
                    if counts {
                        run.pulls = run.pulls.saturating_add(1);
                    }
                    BoxState::Rearming { left: BOX_REARM_SECS }
                }
            }
            BoxState::Rearming { left } => {
                let left = left - dt;
                if left > 0.0 { BoxState::Rearming { left } } else { BoxState::Idle }
            }
            BoxState::Leaving { elapsed } => {
                let elapsed = elapsed + dt;
                if elapsed < LEAVING_SECS {
                    BoxState::Leaving { elapsed }
                } else {
                    let to = next_location(&mut run.bag, BOX_LOCATIONS.len(), mystery_box.location, rng);
                    commands.entity(entity).insert(ColliderDisabled);
                    *visibility = Visibility::Hidden;
                    BoxState::Away { left: BOX_MOVE_SECS, to }
                }
            }
            BoxState::Away { left, to } => {
                let left = left - dt;
                if left > 0.0 {
                    BoxState::Away { left, to }
                } else {
                    mystery_box.location = to;
                    *transform = location_transform(to);
                    commands.entity(entity).remove::<ColliderDisabled>();
                    *visibility = Visibility::Inherited;
                    BoxState::Idle
                }
            }
        };
    }
}

/// Each Box owns its Buyable: a spin while idle, the Prize while one waits, and nothing otherwise.
fn price_boxes(mut commands: Commands, mut boxes: Query<(Entity, &MysteryBox, &Transform, Option<&mut Buyable>)>) {
    for (entity, mystery_box, transform, buyable) in &mut boxes {
        let offer = match mystery_box.state {
            BoxState::Idle => Some(("Press F for Random Weapon".to_string(), Some(mystery_box.price))),
            BoxState::Offering { prize, .. } => Some((format!("Press F to take {}", prize.name()), None)),
            _ => None,
        };
        // The middle of the lid's front edge.
        let use_point = transform.translation + transform.back() * (BOX_SIZE.z / 2.0) + Vec3::Y * (BOX_SIZE.y / 2.0);
        match (offer, buyable) {
            (None, Some(_)) => {
                commands.entity(entity).remove::<Buyable>();
            }
            (Some((prompt, price)), None) => {
                commands.entity(entity).insert(Buyable {
                    use_point,
                    prompt,
                    price,
                    range: USE_RANGE,
                });
            }
            (Some((prompt, price)), Some(mut buyable)) => {
                if buyable.prompt != prompt {
                    buyable.prompt = prompt;
                }
                if buyable.price != price {
                    buyable.price = price;
                }
                if buyable.use_point != use_point {
                    buyable.use_point = use_point;
                }
            }
            (None, None) => {}
        }
    }
}

/// The spinning and waiting Prize, the Teddy Bear's flight, and the Box's shake.
fn animate_boxes(
    time: Res<Time>,
    assets: Res<BoxAssets>,
    boxes: Query<(&MysteryBox, &BoxParts)>,
    mut parts: Query<(&mut Transform, &mut Visibility), Without<MysteryBox>>,
    mut looks: Query<&mut MeshMaterial3d<StandardMaterial>>,
) {
    let now = time.elapsed_secs();
    let palette = assets.prize_mats.len();
    for (mystery_box, box_parts) in &boxes {
        // The Prize: rising while it spins, then sinking back until it's taken or lost.
        let prize = match mystery_box.state {
            BoxState::Spinning { elapsed, .. } => Some((
                PRIZE_LOW.lerp(PRIZE_HIGH, elapsed / BOX_SPIN_SECS),
                elapsed * 6.0,
                (spin_step(elapsed) as usize * 7) % palette,
            )),
            BoxState::Offering { prize: Prize::Weapon(id), left, .. } => Some((
                PRIZE_LOW.lerp(PRIZE_HIGH, left / BOX_PICKUP_SECS),
                0.0,
                id as usize % palette,
            )),
            _ => None,
        };
        if let Ok((mut transform, mut visibility)) = parts.get_mut(box_parts.prize) {
            match prize {
                Some((height, yaw, look)) => {
                    visibility.set_if_neq(Visibility::Inherited);
                    *transform = Transform::from_xyz(0.0, height, 0.0).with_rotation(Quat::from_rotation_y(yaw));
                    if let Ok(mut material) = looks.get_mut(box_parts.prize)
                        && material.0 != assets.prize_mats[look]
                    {
                        material.0 = assets.prize_mats[look].clone();
                    }
                }
                None => {
                    visibility.set_if_neq(Visibility::Hidden);
                }
            }
        }

        let leaving = match mystery_box.state {
            BoxState::Leaving { elapsed } => Some(elapsed),
            _ => None,
        };
        // The Teddy Bear sits on the Box, then flies straight up.
        if let Ok((mut transform, mut visibility)) = parts.get_mut(box_parts.teddy) {
            match leaving.filter(|&elapsed| elapsed < BOX_TEDDY_RISE_SECS + BOX_TEDDY_FLY_SECS) {
                Some(elapsed) => {
                    visibility.set_if_neq(Visibility::Inherited);
                    let flown = (elapsed - BOX_TEDDY_RISE_SECS).max(0.0);
                    *transform = Transform::from_xyz(0.0, PRIZE_HIGH + flown * TEDDY_FLY_SPEED, 0.0)
                        .with_rotation(Quat::from_rotation_y(flown * 4.0));
                }
                None => {
                    visibility.set_if_neq(Visibility::Hidden);
                }
            }
        }
        // Then the Box shakes.
        if let Ok((mut transform, _)) = parts.get_mut(box_parts.body) {
            let shaking = leaving.is_some_and(|elapsed| elapsed >= BOX_TEDDY_RISE_SECS + BOX_TEDDY_FLY_SECS);
            let shake = if shaking {
                Transform::from_xyz((now * 41.0).sin() * 0.04, 0.0, (now * 33.0).cos() * 0.03)
                    .with_rotation(Quat::from_rotation_z((now * 29.0).sin() * 0.03))
            } else {
                Transform::default()
            };
            if *transform != shake {
                *transform = shake;
            }
        }
    }
}

/// Rubble marks every Box Location with no Box standing at it.
fn show_rubble(boxes: Query<&MysteryBox>, mut locations: Query<(&BoxLocation, &mut Visibility)>) {
    for (location, mut visibility) in &mut locations {
        let occupied = boxes.iter().any(|mystery_box| mystery_box.present() && mystery_box.location == location.0);
        visibility.set_if_neq(if occupied { Visibility::Hidden } else { Visibility::Inherited });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn no_teddy_in_the_first_four_pulls_at_a_location() {
        for pulls in 0..=3 {
            assert_eq!(teddy_chance(pulls, true), 0.0);
            assert_eq!(teddy_chance(pulls, false), 0.0);
        }
    }

    #[test]
    fn pulls_four_to_seven_give_a_teddy_15_percent_of_the_time() {
        for pulls in 4..=7 {
            assert_eq!(teddy_chance(pulls, true), 0.15);
            assert_eq!(teddy_chance(pulls, false), 0.15);
        }
    }

    #[test]
    fn the_first_location_always_gives_a_teddy_from_the_eighth_pull() {
        for pulls in [8, 12, 13, 50] {
            assert_eq!(teddy_chance(pulls, true), 1.0);
        }
    }

    #[test]
    fn later_locations_give_a_teddy_30_then_50_percent_of_the_time() {
        for pulls in 8..=12 {
            assert_eq!(teddy_chance(pulls, false), 0.30);
        }
        for pulls in [13, 14, 100] {
            assert_eq!(teddy_chance(pulls, false), 0.50);
        }
    }

    #[test]
    fn a_spin_with_no_teddy_chance_always_gives_a_prize() {
        let pool = box_pool(&[], &[]);
        let rng = &mut StdRng::seed_from_u64(1);
        for _ in 0..200 {
            assert!(matches!(roll_spin(0.0, &pool, rng), Spin::Prize(prize) if pool.contains(&prize)));
        }
        for _ in 0..20 {
            assert_eq!(roll_spin(1.0, &pool, rng), Spin::Teddy);
        }
    }

    #[test]
    fn an_empty_pool_gives_a_teddy_so_the_spin_is_refunded() {
        assert_eq!(roll_spin(0.0, &[], &mut StdRng::seed_from_u64(2)), Spin::Teddy);
    }

    fn weapons(pool: &[Prize]) -> Vec<WeaponId> {
        pool.iter().map(|&Prize::Weapon(id)| id).collect()
    }

    #[test]
    fn the_pool_holds_the_ordinary_box_weapons_only() {
        let pool = weapons(&box_pool(&[], &[]));
        assert_eq!(pool.len(), 18);
        for id in [WeaponId::M1911, WeaponId::M14, WeaponId::Olympia, WeaponId::Mp40, WeaponId::TrenchGun, WeaponId::Ak74u] {
            assert!(!pool.contains(&id), "{id:?} is never in the Box");
        }
        assert!(pool.contains(&WeaponId::Galil));
        assert!(pool.contains(&WeaponId::ChinaLake));
    }

    #[test]
    fn the_pool_skips_weapons_the_player_holds() {
        let pool = weapons(&box_pool(&[WeaponId::M1911, WeaponId::Hk21], &[]));
        assert!(!pool.contains(&WeaponId::Hk21));
        assert_eq!(pool.len(), 17);
        let pool = weapons(&box_pool(&[WeaponId::Spas12, WeaponId::Python], &[]));
        assert!(!pool.contains(&WeaponId::Spas12) && !pool.contains(&WeaponId::Python));
        assert_eq!(pool.len(), 16);
    }

    #[test]
    fn the_pool_skips_a_limited_weapon_already_in_play() {
        // The limited Weapons wait for their own slice, but the rule is already in place.
        let limited = WeaponId::Thundergun;
        assert_eq!(limited.stats().availability, Availability::LimitedBox);
        assert!(!weapons(&box_pool(&[], &[limited])).contains(&limited));
        // An ordinary Box Weapon in another Box can still come up.
        assert!(weapons(&box_pool(&[], &[WeaponId::Galil])).contains(&WeaponId::Galil));
    }

    #[test]
    fn a_fresh_list_never_puts_the_current_location_first() {
        for seed in 0..500 {
            let rng = &mut StdRng::seed_from_u64(seed);
            for current in 0..4 {
                let bag = shuffled_locations(4, current, rng);
                assert_ne!(bag[0], current);
                let mut sorted = bag.clone();
                sorted.sort();
                assert_eq!(sorted, [0, 1, 2, 3], "every location once");
            }
        }
    }

    #[test]
    fn the_box_never_stays_put_and_visits_every_location_before_repeating() {
        for seed in 0..200 {
            let rng = &mut StdRng::seed_from_u64(seed);
            let mut current = 0;
            let mut bag = shuffled_locations(4, current, rng);
            let mut visits = Vec::new();
            for _ in 0..12 {
                let next = next_location(&mut bag, 4, current, rng);
                assert_ne!(next, current, "seed {seed}");
                visits.push(next);
                current = next;
            }
            // Three full lists of four: each holds every location once.
            for list in visits.chunks(4) {
                let mut sorted = list.to_vec();
                sorted.sort();
                assert_eq!(sorted, [0, 1, 2, 3], "seed {seed}");
            }
        }
    }

    #[test]
    fn the_list_reshuffles_when_empty() {
        let rng = &mut StdRng::seed_from_u64(7);
        let mut bag = Vec::new();
        let next = next_location(&mut bag, 4, 2, rng);
        assert_ne!(next, 2);
        assert_eq!(bag.len(), 3);
    }

    #[test]
    fn the_spin_slows_over_its_steps() {
        assert_eq!(spin_step(0.0), 0);
        assert_eq!(spin_step(BOX_SPIN_SECS), BOX_SPIN_STEPS);
        // Half the steps go by in the first quarter of the spin.
        assert_eq!(spin_step(BOX_SPIN_SECS / 4.0), BOX_SPIN_STEPS / 2);
    }
}

