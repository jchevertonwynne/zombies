//! Equipment: frags (the Lethal), the Monkey Bomb (the Tactical), Claymores (the Mine) and the
//! Bowie Knife, with their wall buys and Round refills.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::ammo_mod::AmmoMods;
use crate::buy::{Buyable, Purchased, Refused};
use crate::explosive::{Explosion, in_sight, world_filter};
use crate::hud::text;
use crate::layers::GameLayer;
use crate::player::{Player, PlayerCamera};
use crate::round::{Round, RoundStarted};
use crate::state::{GameState, InRun, Pause};
use crate::tuning::*;
use crate::weapon::Knife;
use crate::zombie::{Dying, Lured, Zombie};
use crate::zone::ZoneId;

/// The Equipment the Player carries. It never counts toward the Weapon limit.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct Equipment {
    pub frags: u32,
    /// Whether the Claymore wall has been bought; from then on they refill every Round.
    pub claymores_owned: bool,
    pub claymores: u32,
    /// Whether the Monkey Bomb has come from the Mystery Box, so Max Ammo refills it.
    pub monkey_bombs_owned: bool,
    pub monkey_bombs: u32,
    /// Seconds a frag has been cooking in the hand, while G is held.
    pub cooking: Option<f32>,
}

impl Default for Equipment {
    fn default() -> Self {
        Self {
            frags: FRAG_START,
            claymores_owned: false,
            claymores: 0,
            monkey_bombs_owned: false,
            monkey_bombs: 0,
            cooking: None,
        }
    }
}

/// Frags after a new Round's restock: +2, up to the most carried.
pub fn frags_after_round(frags: u32) -> u32 {
    frags.saturating_add(FRAG_PER_ROUND).min(FRAG_MAX).max(frags)
}

/// Claymores after a new Round's restock: back to 2 once the wall is bought, otherwise none.
pub fn claymores_after_round(owned: bool, claymores: u32) -> u32 {
    if owned { claymores.max(CLAYMORE_PER_ROUND) } else { claymores }
}

/// What a cooking frag does this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cook {
    /// Still held, with this long cooked.
    Held(f32),
    /// Thrown with this much fuse left.
    Thrown(f32),
    /// Held too long: it goes off in the hand.
    InHand,
}

/// Advances a frag cooked for `cooked` seconds by `dt`, given whether G is still held.
pub fn cook(cooked: f32, dt: f32, held: bool) -> Cook {
    let cooked = cooked + dt;
    if cooked >= FRAG_FUSE_SECS {
        Cook::InHand
    } else if held {
        Cook::Held(cooked)
    } else {
        Cook::Thrown(FRAG_FUSE_SECS - cooked)
    }
}

impl Equipment {
    /// A new Round restocks frags by 2 up to 4, and Claymores to 2 once bought. The Monkey Bomb
    /// never refills by Round. The first Round adds nothing to the starting frags.
    pub fn start_round(&mut self, round: u32) {
        if round <= 1 {
            return;
        }
        self.frags = frags_after_round(self.frags);
        self.claymores = claymores_after_round(self.claymores_owned, self.claymores);
    }

    /// Max Ammo refills every piece of Equipment the Player has.
    // Power-ups (Max Ammo) arrive in their own slice; this is the hook it calls.
    #[allow(dead_code)]
    pub fn max_ammo(&mut self) {
        self.frags = self.frags.max(FRAG_MAX);
        if self.claymores_owned {
            self.claymores = self.claymores.max(CLAYMORE_PER_ROUND);
        }
        if self.monkey_bombs_owned {
            self.monkey_bombs = self.monkey_bombs.max(MONKEY_BOMB_CARRY);
        }
    }

    /// Taking the Monkey Bomb from the Mystery Box fills the Tactical slot to 3.
    pub fn take_monkey_bombs(&mut self) {
        self.monkey_bombs_owned = true;
        self.monkey_bombs = self.monkey_bombs.max(MONKEY_BOMB_CARRY);
    }

    /// Whether the Mystery Box may offer the Monkey Bomb: it's skipped while any are left.
    // The Mystery Box calls this when it builds its pool.
    #[allow(dead_code)]
    pub fn monkey_bomb_in_pool(&self) -> bool {
        self.monkey_bombs == 0
    }
}

/// The Equipment sold on the walls.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum EquipmentWall {
    Frags,
    Claymore,
    BowieKnife,
}

impl EquipmentWall {
    fn name(self) -> &'static str {
        match self {
            EquipmentWall::Frags => "Frag Grenades",
            EquipmentWall::Claymore => "Claymores",
            EquipmentWall::BowieKnife => "Bowie Knife",
        }
    }

    fn price(self) -> u64 {
        match self {
            EquipmentWall::Frags => FRAG_WALL_PRICE,
            EquipmentWall::Claymore => CLAYMORE_WALL_PRICE,
            EquipmentWall::BowieKnife => BOWIE_KNIFE_WALL_PRICE,
        }
    }
}

/// What an Equipment wall sells now: its price and whether it's refused, or `None` once a
/// one-time buy is owned and it shows no Prompt.
pub fn equipment_offer(wall: EquipmentWall, equipment: &Equipment, bowie: bool) -> Option<(u64, bool)> {
    match wall {
        EquipmentWall::Frags => Some((wall.price(), equipment.frags >= FRAG_MAX)),
        EquipmentWall::Claymore => (!equipment.claymores_owned).then_some((wall.price(), false)),
        EquipmentWall::BowieKnife => (!bowie).then_some((wall.price(), false)),
    }
}

/// Gives what an Equipment wall sells.
pub fn buy_equipment(wall: EquipmentWall, equipment: &mut Equipment, bowie: &mut bool) {
    match wall {
        EquipmentWall::Frags => equipment.frags = equipment.frags.max(FRAG_MAX),
        EquipmentWall::Claymore => {
            equipment.claymores_owned = true;
            equipment.claymores = equipment.claymores.max(CLAYMORE_PER_ROUND);
        }
        EquipmentWall::BowieKnife => *bowie = true,
    }
}

/// Whether a Zombie at `target` sets off a Claymore at `claymore` facing `facing`: within the
/// trigger range and inside the front cone, both measured flat. Line of sight is checked apart.
pub fn claymore_triggered(claymore: Vec3, facing: Vec3, target: Vec3) -> bool {
    let to_target = (target - claymore).with_y(0.0);
    let facing = facing.with_y(0.0);
    if to_target.length() > CLAYMORE_TRIGGER_RANGE {
        return false;
    }
    if to_target.length_squared() < 1e-6 {
        return true;
    }
    facing.angle_between(to_target).to_degrees() <= CLAYMORE_TRIGGER_CONE_DEGREES / 2.0
}

/// A Zombie a Monkey Bomb might pull.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LureCandidate<Z> {
    pub zombie: Z,
    pub distance: f32,
    /// A Zombie tearing at a Barrier keeps tearing instead.
    pub at_barrier: bool,
}

/// The Zombies a Monkey Bomb pulls: the nearest 96 within 39 m, skipping any at a Barrier.
pub fn lured<Z>(candidates: impl IntoIterator<Item = LureCandidate<Z>>) -> Vec<Z> {
    let mut near: Vec<_> = candidates
        .into_iter()
        .filter(|c| !c.at_barrier && c.distance <= MONKEY_BOMB_LURE_RANGE)
        .collect();
    near.sort_by(|a, b| a.distance.total_cmp(&b.distance));
    near.into_iter().take(MONKEY_BOMB_LURE_MAX).map(|c| c.zombie).collect()
}

/// A thrown frag, burning down its fuse.
#[derive(Component)]
struct Frag {
    fuse: f32,
}

/// A thrown Monkey Bomb. Its fuse starts when it lands.
#[derive(Component)]
struct MonkeyBomb {
    fuse: Option<f32>,
    /// Seconds since it was thrown.
    age: f32,
}

/// A planted Claymore, and the countdown once a Zombie sets it off.
#[derive(Component)]
struct Claymore {
    facing: Vec3,
    countdown: Option<f32>,
}

#[derive(Component)]
struct EquipmentText;

#[derive(Resource)]
struct EquipmentAssets {
    frag: Handle<Mesh>,
    frag_mat: Handle<StandardMaterial>,
    monkey: Handle<Mesh>,
    monkey_mat: Handle<StandardMaterial>,
    claymore: Handle<Mesh>,
    claymore_mat: Handle<StandardMaterial>,
}

/// Where each Equipment wall's chalk sits: its Zone, the centre on a wall's face (x, y, z), and
/// which way that face looks.
const EQUIPMENT_WALLS: [(EquipmentWall, ZoneId, Vec3, Vec3); 3] = [
    // On the Dining Room's outer west wall.
    (EquipmentWall::Frags, ZoneId::DiningRoom, Vec3::new(-25.8, 1.5, -20.0), Vec3::X),
    // On the Power Room's outer east wall.
    (EquipmentWall::Claymore, ZoneId::PowerRoom, Vec3::new(25.8, 1.5, -20.0), Vec3::NEG_X),
    // On the full wall at the Balcony's south end, a storey up.
    (EquipmentWall::BowieKnife, ZoneId::Balcony, Vec3::new(-12.0, 5.5, -13.8), Vec3::Z),
];

pub struct EquipmentPlugin;

impl Plugin for EquipmentPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_assets)
            .add_systems(OnEnter(GameState::Playing), (spawn_walls, spawn_text))
            .add_systems(
                Update,
                (
                    restock,
                    (buy_walls, price_walls).chain(),
                    (throw_frags, throw_monkey_bombs, plant_claymores),
                    (burn_frags, land_monkey_bombs, lure_zombies, trigger_claymores),
                    equipment_text,
                )
                    .chain()
                    .run_if(in_state(Pause::Running)),
            );
    }
}

fn load_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(EquipmentAssets {
        frag: meshes.add(Sphere::new(THROWN_RADIUS)),
        frag_mat: materials.add(Color::srgb(0.25, 0.3, 0.2)),
        monkey: meshes.add(Cuboid::from_size(Vec3::splat(THROWN_RADIUS * 2.0))),
        monkey_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.6, 0.35, 0.15),
            emissive: LinearRgba::rgb(3.0, 0.4, 0.1),
            ..default()
        }),
        claymore: meshes.add(Cuboid::new(0.22, 0.12, 0.05)),
        claymore_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.25, 0.3, 0.15),
            emissive: LinearRgba::rgb(0.0, 1.5, 0.0),
            ..default()
        }),
    });
}

/// Each Equipment wall: a chalk outline with its Buyable just in front.
fn spawn_walls(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let chalk = materials.add(StandardMaterial {
        base_color: Color::srgb(0.9, 0.9, 0.85),
        perceptual_roughness: 1.0,
        ..default()
    });
    for (wall, _, centre, normal) in EQUIPMENT_WALLS {
        let size = match wall {
            EquipmentWall::Frags => Vec3::new(0.3, 0.4, 0.02),
            EquipmentWall::Claymore => Vec3::new(0.5, 0.3, 0.02),
            EquipmentWall::BowieKnife => Vec3::new(0.7, 0.1, 0.02),
        };
        commands.spawn((
            InRun,
            wall,
            Buyable {
                use_point: centre + normal * 0.05,
                prompt: String::new(),
                price: Some(wall.price()),
                range: USE_RANGE,
            },
            Mesh3d(meshes.add(Cuboid::from_size(size))),
            MeshMaterial3d(chalk.clone()),
            Transform::from_translation(centre + normal * 0.01).with_rotation(Quat::from_rotation_arc(Vec3::Z, normal)),
        ));
    }
}

fn spawn_text(mut commands: Commands) {
    commands.spawn((
        InRun,
        EquipmentText,
        text("", 22.0, Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            // Above the Round number.
            left: px(32.0),
            bottom: px(124.0),
            ..default()
        },
    ));
}

fn equipment_text(
    player: Single<(&Equipment, &Knife), With<Player>>,
    mut text: Single<&mut Text, With<EquipmentText>>,
) {
    let (equipment, knife) = *player;
    let mut line = format!("Frags {}", equipment.frags);
    if equipment.cooking.is_some() {
        line.push_str(" (cooking)");
    }
    if equipment.monkey_bombs_owned {
        line.push_str(&format!("   Monkey Bombs {}", equipment.monkey_bombs));
    }
    if equipment.claymores_owned {
        line.push_str(&format!("   Claymores {}", equipment.claymores));
    }
    if knife.bowie {
        line.push_str("   Bowie Knife");
    }
    if text.0 != line {
        text.0 = line;
    }
}

/// Each new Round restocks the Player's Equipment.
fn restock(
    mut started: MessageReader<RoundStarted>,
    round: Option<Res<Round>>,
    mut equipment: Single<&mut Equipment, With<Player>>,
) {
    if started.read().count() == 0 {
        return;
    }
    if let Some(round) = round {
        equipment.start_round(round.number);
    }
}

/// Each Equipment wall owns its Buyable: it keeps the price and refusal current, and stops
/// offering a one-time buy once it's owned.
fn price_walls(
    mut commands: Commands,
    player: Single<(&Equipment, &Knife), With<Player>>,
    mut walls: Query<(Entity, &EquipmentWall, &mut Buyable, Has<Refused>)>,
) {
    let (equipment, knife) = *player;
    for (entity, wall, mut buyable, was_refused) in &mut walls {
        let Some((price, refused)) = equipment_offer(*wall, equipment, knife.bowie) else {
            commands.entity(entity).remove::<(Buyable, Refused)>();
            continue;
        };
        let prompt = format!("Press F to buy {}", wall.name());
        if buyable.prompt != prompt {
            buyable.prompt = prompt;
        }
        if buyable.price != Some(price) {
            buyable.price = Some(price);
        }
        match (refused, was_refused) {
            (true, false) => {
                commands.entity(entity).insert(Refused);
            }
            (false, true) => {
                commands.entity(entity).remove::<Refused>();
            }
            _ => {}
        }
    }
}

fn buy_walls(
    mut purchases: MessageReader<Purchased>,
    walls: Query<&EquipmentWall>,
    player: Single<(&mut Equipment, &mut Knife), With<Player>>,
) {
    let (mut equipment, mut knife) = player.into_inner();
    for Purchased(entity) in purchases.read() {
        if let Ok(wall) = walls.get(*entity) {
            buy_equipment(*wall, &mut equipment, &mut knife.bowie);
        }
    }
}

/// A thrown frag or Monkey Bomb: it bounces off the world and passes through everyone.
fn thrown(mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>, camera: &GlobalTransform) -> impl Bundle {
    let forward = camera.forward();
    (
        InRun,
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material.clone()),
        Transform::from_translation(camera.translation() + *forward * 0.4),
        RigidBody::Dynamic,
        Collider::sphere(THROWN_RADIUS),
        CollisionLayers::new(GameLayer::Thrown, GameLayer::World),
        Restitution::new(THROWN_RESTITUTION),
        Friction::new(0.8),
        AngularDamping(2.0),
        LinearVelocity(*forward * EQUIPMENT_THROW_SPEED + Vec3::Y * EQUIPMENT_THROW_LIFT),
        TranslationInterpolation,
    )
}

/// G pulls a frag's pin. Holding G cooks it, and letting go throws it with what's left of the
/// fuse. Held too long, it goes off in the hand.
fn throw_frags(
    mut commands: Commands,
    mut equipment: Single<&mut Equipment, With<Player>>,
    camera: Single<&GlobalTransform, With<PlayerCamera>>,
    input: Res<ButtonInput<KeyCode>>,
    assets: Res<EquipmentAssets>,
    mut explosions: MessageWriter<Explosion>,
    time: Res<Time>,
) {
    let held = input.pressed(KeyCode::KeyG);
    let Some(cooked) = equipment.cooking else {
        if input.just_pressed(KeyCode::KeyG) && equipment.frags > 0 {
            equipment.frags -= 1;
            equipment.cooking = Some(0.0);
        }
        return;
    };
    match cook(cooked, time.delta_secs(), held) {
        Cook::Held(cooked) => equipment.cooking = Some(cooked),
        Cook::Thrown(fuse) => {
            equipment.cooking = None;
            commands.spawn((Frag { fuse }, thrown(&assets.frag, &assets.frag_mat, &camera)));
        }
        Cook::InHand => {
            equipment.cooking = None;
            explosions.write(Explosion {
                at: camera.translation(),
                splash: FRAG_SPLASH,
                hurts_player: true,
                ammo_mods: AmmoMods::NONE,
            });
        }
    }
}

/// 4 throws a Monkey Bomb.
fn throw_monkey_bombs(
    mut commands: Commands,
    mut equipment: Single<&mut Equipment, With<Player>>,
    camera: Single<&GlobalTransform, With<PlayerCamera>>,
    input: Res<ButtonInput<KeyCode>>,
    assets: Res<EquipmentAssets>,
) {
    if !input.just_pressed(KeyCode::Digit4) || equipment.monkey_bombs == 0 {
        return;
    }
    equipment.monkey_bombs -= 1;
    commands.spawn((
        MonkeyBomb { fuse: None, age: 0.0 },
        thrown(&assets.monkey, &assets.monkey_mat, &camera),
    ));
}

/// 5 plants a Claymore on the floor at the Player's feet, facing where they look.
fn plant_claymores(
    mut commands: Commands,
    player: Single<(&mut Equipment, &Transform), With<Player>>,
    input: Res<ButtonInput<KeyCode>>,
    spatial: SpatialQuery,
    assets: Res<EquipmentAssets>,
) {
    let (mut equipment, transform) = player.into_inner();
    if !input.just_pressed(KeyCode::Digit5) || equipment.claymores == 0 {
        return;
    }
    let Some(floor) = spatial.cast_ray(transform.translation, Dir3::NEG_Y, 3.0, true, &world_filter()) else {
        return;
    };
    equipment.claymores -= 1;
    let facing = transform.forward().with_y(0.0).normalize_or(Vec3::NEG_Z);
    let at = transform.translation - Vec3::Y * floor.distance + facing * 0.5 + Vec3::Y * 0.08;
    commands.spawn((
        InRun,
        Claymore { facing, countdown: None },
        Mesh3d(assets.claymore.clone()),
        MeshMaterial3d(assets.claymore_mat.clone()),
        Transform::from_translation(at).looking_to(facing, Vec3::Y),
    ));
}

fn burn_frags(
    mut commands: Commands,
    mut frags: Query<(Entity, &mut Frag, &Transform)>,
    mut explosions: MessageWriter<Explosion>,
    time: Res<Time>,
) {
    for (entity, mut frag, transform) in &mut frags {
        frag.fuse -= time.delta_secs();
        if frag.fuse <= 0.0 {
            explosions.write(Explosion {
                at: transform.translation + Vec3::Y * 0.1,
                splash: FRAG_SPLASH,
                hurts_player: true,
                ammo_mods: AmmoMods::NONE,
            });
            commands.entity(entity).despawn();
        }
    }
}

/// A Monkey Bomb's fuse starts when it lands, and it goes off when the fuse runs out.
fn land_monkey_bombs(
    mut commands: Commands,
    mut bombs: Query<(Entity, &mut MonkeyBomb, &Transform)>,
    spatial: SpatialQuery,
    mut explosions: MessageWriter<Explosion>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    for (entity, mut bomb, transform) in &mut bombs {
        bomb.age += dt;
        match bomb.fuse {
            None => {
                let landed = spatial
                    .cast_ray(transform.translation, Dir3::NEG_Y, THROWN_RADIUS + 0.05, true, &world_filter())
                    .is_some();
                if landed {
                    bomb.fuse = Some(MONKEY_BOMB_FUSE_SECS);
                } else if bomb.age >= MONKEY_BOMB_LOST_SECS {
                    commands.entity(entity).despawn();
                }
            }
            Some(left) if left - dt <= 0.0 => {
                explosions.write(Explosion {
                    at: transform.translation + Vec3::Y * 0.1,
                    splash: MONKEY_BOMB_SPLASH,
                    hurts_player: true,
                    ammo_mods: AmmoMods::NONE,
                });
                commands.entity(entity).despawn();
            }
            Some(left) => bomb.fuse = Some(left - dt),
        }
    }
}

/// Every landed Monkey Bomb pulls the Zombies near it, the first to land getting first pick.
fn lure_zombies(
    mut commands: Commands,
    bombs: Query<(&MonkeyBomb, &Transform)>,
    zombies: Query<(Entity, &Transform, Option<&Lured>), (With<Zombie>, Without<Dying>)>,
) {
    let mut active: Vec<_> = bombs
        .iter()
        .filter_map(|(bomb, transform)| bomb.fuse.map(|fuse| (fuse, transform.translation)))
        .collect();
    // The least fuse left landed first.
    active.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut targets: Vec<(Entity, Vec3)> = Vec::new();
    for (_, at) in active {
        let candidates = zombies
            .iter()
            .filter(|(entity, ..)| !targets.iter().any(|(taken, _)| taken == entity))
            .map(|(entity, transform, _)| LureCandidate {
                zombie: entity,
                distance: transform.translation.distance(at),
                // Barriers arrive in their own slice; Zombies tearing at one must say so here.
                at_barrier: false,
            });
        targets.extend(lured(candidates).into_iter().map(|zombie| (zombie, at)));
    }

    for (entity, _, lure) in &zombies {
        let target = targets.iter().find(|(zombie, _)| *zombie == entity).map(|(_, at)| *at);
        match (target, lure) {
            (Some(at), Some(lure)) if lure.0 == at => {}
            (Some(at), _) => {
                commands.entity(entity).insert(Lured(at));
            }
            (None, Some(_)) => {
                commands.entity(entity).remove::<Lured>();
            }
            (None, None) => {}
        }
    }
}

/// A Zombie in a Claymore's front cone and in sight sets it off; it blows 0.4 s later and never
/// hurts the Player.
fn trigger_claymores(
    mut commands: Commands,
    mut claymores: Query<(Entity, &mut Claymore, &Transform)>,
    zombies: Query<&GlobalTransform, (With<Zombie>, Without<Dying>)>,
    spatial: SpatialQuery,
    mut explosions: MessageWriter<Explosion>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    for (entity, mut claymore, transform) in &mut claymores {
        let at = transform.translation;
        match claymore.countdown {
            None => {
                // Line of sight is taken from just above the Claymore to the Zombie's middle.
                let eye = at + Vec3::Y * 0.3;
                let set_off = zombies.iter().any(|zombie| {
                    let target = zombie.translation();
                    claymore_triggered(at, claymore.facing, target) && in_sight(&spatial, eye, target)
                });
                if set_off {
                    claymore.countdown = Some(CLAYMORE_DELAY_SECS);
                }
            }
            Some(left) if left - dt <= 0.0 => {
                explosions.write(Explosion {
                    at: at + Vec3::Y * 0.3,
                    splash: CLAYMORE_SPLASH,
                    hurts_player: false,
                    ammo_mods: AmmoMods::NONE,
                });
                commands.entity(entity).despawn();
            }
            Some(left) => claymore.countdown = Some(left - dt),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_player_starts_with_two_frags_and_nothing_else() {
        let equipment = Equipment::default();
        assert_eq!(equipment.frags, 2);
        assert_eq!((equipment.claymores, equipment.monkey_bombs), (0, 0));
    }

    #[test]
    fn frags_restock_by_two_a_round_up_to_four() {
        assert_eq!(frags_after_round(0), 2);
        assert_eq!(frags_after_round(1), 3);
        assert_eq!(frags_after_round(2), 4);
        assert_eq!(frags_after_round(3), 4);
        assert_eq!(frags_after_round(4), 4);
    }

    #[test]
    fn the_first_round_adds_no_frags() {
        let mut equipment = Equipment::default();
        equipment.start_round(1);
        assert_eq!(equipment.frags, 2);
        equipment.start_round(2);
        assert_eq!(equipment.frags, 4);
    }

    #[test]
    fn claymores_refill_to_two_every_round_once_bought() {
        assert_eq!(claymores_after_round(false, 0), 0);
        assert_eq!(claymores_after_round(true, 0), 2);
        assert_eq!(claymores_after_round(true, 1), 2);
        assert_eq!(claymores_after_round(true, 2), 2);
    }

    #[test]
    fn monkey_bombs_never_refill_by_round() {
        let mut equipment = Equipment::default();
        equipment.take_monkey_bombs();
        equipment.monkey_bombs = 1;
        equipment.start_round(5);
        assert_eq!(equipment.monkey_bombs, 1);
    }

    #[test]
    fn the_box_fills_the_tactical_slot_to_three_and_skips_it_while_any_are_left() {
        let mut equipment = Equipment::default();
        assert!(equipment.monkey_bomb_in_pool());
        equipment.take_monkey_bombs();
        assert_eq!(equipment.monkey_bombs, 3);
        assert!(!equipment.monkey_bomb_in_pool());
        equipment.monkey_bombs = 1;
        assert!(!equipment.monkey_bomb_in_pool());
        equipment.monkey_bombs = 0;
        assert!(equipment.monkey_bomb_in_pool());
    }

    #[test]
    fn max_ammo_refills_the_equipment_the_player_has() {
        let mut equipment = Equipment {
            frags: 0,
            ..Equipment::default()
        };
        equipment.max_ammo();
        assert_eq!((equipment.frags, equipment.claymores, equipment.monkey_bombs), (4, 0, 0));

        let mut bought = Equipment {
            frags: 1,
            claymores_owned: true,
            claymores: 0,
            monkey_bombs_owned: true,
            monkey_bombs: 0,
            cooking: None,
        };
        bought.max_ammo();
        assert_eq!((bought.frags, bought.claymores, bought.monkey_bombs), (4, 2, 3));
    }

    #[test]
    fn a_frag_cooks_while_held_and_keeps_the_rest_of_its_fuse() {
        assert_eq!(cook(0.0, 1.0, true), Cook::Held(1.0));
        assert_eq!(cook(1.0, 1.0, false), Cook::Thrown(1.5));
        assert_eq!(cook(0.0, 0.0, false), Cook::Thrown(3.5));
        assert_eq!(cook(3.0, 0.5, true), Cook::InHand);
        assert_eq!(cook(3.4, 0.2, false), Cook::InHand, "too late to throw");
    }

    #[test]
    fn the_frag_wall_fills_to_four_and_refuses_when_full() {
        let mut equipment = Equipment::default();
        assert_eq!(equipment_offer(EquipmentWall::Frags, &equipment, false), Some((250, false)));
        let mut bowie = false;
        buy_equipment(EquipmentWall::Frags, &mut equipment, &mut bowie);
        assert_eq!(equipment.frags, 4);
        assert_eq!(equipment_offer(EquipmentWall::Frags, &equipment, false), Some((250, true)));
    }

    #[test]
    fn claymores_and_the_bowie_are_one_time_buys() {
        let mut equipment = Equipment::default();
        let mut bowie = false;
        assert_eq!(equipment_offer(EquipmentWall::Claymore, &equipment, bowie), Some((1000, false)));
        assert_eq!(equipment_offer(EquipmentWall::BowieKnife, &equipment, bowie), Some((3000, false)));

        buy_equipment(EquipmentWall::Claymore, &mut equipment, &mut bowie);
        buy_equipment(EquipmentWall::BowieKnife, &mut equipment, &mut bowie);
        assert_eq!(equipment.claymores, 2);
        assert!(bowie);
        assert_eq!(equipment_offer(EquipmentWall::Claymore, &equipment, bowie), None);
        assert_eq!(equipment_offer(EquipmentWall::BowieKnife, &equipment, bowie), None);
    }

    #[test]
    fn a_claymore_triggers_in_its_front_cone_within_range() {
        let at = Vec3::ZERO;
        let facing = Vec3::NEG_Z;
        assert!(claymore_triggered(at, facing, Vec3::new(0.0, 0.8, -2.0)));
        assert!(!claymore_triggered(at, facing, Vec3::new(0.0, 0.8, -2.5)), "out of range");
        assert!(!claymore_triggered(at, facing, Vec3::new(0.0, 0.8, 1.0)), "behind it");
        // 35° either side of straight ahead.
        let edge = |degrees: f32| Vec3::new(degrees.to_radians().sin() * 2.0, 0.0, -degrees.to_radians().cos() * 2.0);
        assert!(claymore_triggered(at, facing, edge(34.0)));
        assert!(!claymore_triggered(at, facing, edge(36.0)));
    }

    fn candidate(zombie: u32, distance: f32) -> LureCandidate<u32> {
        LureCandidate {
            zombie,
            distance,
            at_barrier: false,
        }
    }

    #[test]
    fn a_monkey_bomb_pulls_zombies_within_39_m() {
        let pulled = lured([candidate(0, 10.0), candidate(1, 39.0), candidate(2, 40.0)]);
        assert_eq!(pulled, [0, 1]);
    }

    #[test]
    fn a_monkey_bomb_pulls_at_most_the_nearest_96() {
        let pulled = lured((0..120).rev().map(|i| candidate(i, i as f32 * 0.3)));
        assert_eq!(pulled.len(), 96);
        assert!(pulled.iter().all(|&i| i < 96));
    }

    #[test]
    fn zombies_at_barriers_keep_tearing() {
        let tearing = LureCandidate {
            zombie: 0,
            distance: 1.0,
            at_barrier: true,
        };
        assert_eq!(lured([tearing, candidate(1, 5.0)]), [1]);
    }

    #[test]
    fn every_equipment_wall_sits_in_its_zone() {
        for (wall, zone, at, _) in EQUIPMENT_WALLS {
            let (lo, hi) = zone.volume();
            let inside = (lo.x..=hi.x).contains(&at.x) && (lo.y..=hi.y).contains(&at.y) && (lo.z..=hi.z).contains(&at.z);
            assert!(inside, "{wall:?} is outside {}", zone.name());
        }
    }
}
