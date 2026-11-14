//! Equipment: frags (the Lethal), the Monkey Bomb (the Tactical), Claymores (the Mine) and the
//! Bowie Knife, with their wall buys and Round refills.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::authority::HostSimulation;
use crate::maps::darkness::Chalk;
use crate::players::controls::{Action, Controls};
use crate::weapons::ammo_mod::AmmoMods;
use crate::maps::barrier::Entering;
use crate::economy::buy::{Buyable, Purchased, Refused};
use crate::weapons::gun_model::Handheld;
use crate::players::hands::{cook_pose, cook_raise, plant_pose, throw_pose};
use crate::weapons::explosive::{Explosion, Explosive, Launch, Thrown, in_sight, makes_crawlers, world_filter};
use crate::layers::GameLayer;
use crate::weapons::arsenal::Splash;
use crate::modifier::{Modifier, Modifiers, Stat, resolve, resolve_count};
use crate::players::player::{LocalPlayer, Player, PlayerCamera, not_changing_prone};
use crate::run::round::{Round, RoundStarted};
use crate::enemies::sapper::Sapper;
use crate::state::{GameState, InRun, input_runs, world_runs};
use crate::tuning::*;
use crate::weapons::weapon::Knife;
use crate::enemies::zombie::{Dying, Lured, Zombie};
use crate::maps::map::{DAM_X, MapId, QUARRY_X, STOREY, VILLAGE_X};
use crate::maps::zone::ZoneId;

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
    /// The most frags carried and how many a new Round restocks, kept resolved from the Player's
    /// modifier entries.
    pub frag_limits: FragLimits,
    /// Whether the Host has given or refilled some since it last told this Player's game.
    pub host_dirty: bool,
}

/// The most frags carried, and how many each new Round restocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FragLimits {
    pub max: u32,
    pub per_round: u32,
}

impl FragLimits {
    /// The limits after the Player's entries. With none, 2 a Round up to 4.
    pub fn resolve(entries: &[Modifier]) -> Self {
        Self {
            max: resolve_count(Stat::LethalCapacity, FRAG_MAX, entries),
            per_round: resolve_count(Stat::LethalRestock, FRAG_PER_ROUND, entries),
        }
    }
}

impl Default for FragLimits {
    fn default() -> Self {
        Self::resolve(&[])
    }
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
            frag_limits: FragLimits::default(),
            host_dirty: false,
        }
    }
}

/// Frags after a new Round's restock: +2, up to the most carried.
pub fn frags_after_round(frags: u32, limits: FragLimits) -> u32 {
    frags.saturating_add(limits.per_round).min(limits.max).max(frags)
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
        self.frags = frags_after_round(self.frags, self.frag_limits);
        self.claymores = claymores_after_round(self.claymores_owned, self.claymores);
    }

    /// Max Ammo refills every piece of Equipment the Player has.
    pub fn max_ammo(&mut self) {
        self.host_dirty = true;
        self.frags = self.frags.max(self.frag_limits.max);
        if self.claymores_owned {
            self.claymores = self.claymores.max(CLAYMORE_PER_ROUND);
        }
        if self.monkey_bombs_owned {
            self.monkey_bombs = self.monkey_bombs.max(MONKEY_BOMB_CARRY);
        }
    }

    /// Taking the Monkey Bomb from the Mystery Box fills the Tactical slot to 3.
    pub fn take_monkey_bombs(&mut self) {
        self.host_dirty = true;
        self.monkey_bombs_owned = true;
        self.monkey_bombs = self.monkey_bombs.max(MONKEY_BOMB_CARRY);
    }

    /// Whether the Mystery Box may offer the Monkey Bomb: it's skipped while any are left.
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
        EquipmentWall::Frags => Some((wall.price(), equipment.frags >= equipment.frag_limits.max)),
        EquipmentWall::Claymore => (!equipment.claymores_owned).then_some((wall.price(), false)),
        EquipmentWall::BowieKnife => (!bowie).then_some((wall.price(), false)),
    }
}

/// Gives what an Equipment wall sells.
pub fn buy_equipment(wall: EquipmentWall, equipment: &mut Equipment, bowie: &mut bool) {
    equipment.host_dirty = true;
    match wall {
        EquipmentWall::Frags => equipment.frags = equipment.frags.max(equipment.frag_limits.max),
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

/// Anything else that pulls Zombies like a landed Monkey Bomb, such as the Awful Lawton's bolt.
/// Its owner keeps `at` (on the floor) and `left` (the fuse) up to date.
#[derive(Component, Debug, Clone, Copy)]
pub struct Pull {
    pub at: Vec3,
    pub left: f32,
}

/// A thrown frag, burning down its fuse.
#[derive(Component)]
struct Frag {
    /// The Player who threw it.
    thrower: Entity,
    fuse: f32,
}

/// A thrown Monkey Bomb. Its fuse starts when it lands.
#[derive(Component)]
pub struct MonkeyBomb {
    /// The Player who threw it.
    thrower: Entity,
    fuse: Option<f32>,
    /// Seconds since it was thrown.
    age: f32,
}

impl MonkeyBomb {
    /// Whether it has landed and its fuse is burning.
    pub fn landed(&self) -> bool {
        self.fuse.is_some()
    }
}

/// A planted Claymore, and the countdown once a Zombie sets it off.
#[derive(Component)]
pub struct Claymore {
    /// The Player who planted it.
    thrower: Entity,
    facing: Vec3,
    countdown: Option<f32>,
}

impl Claymore {
    /// The Player who planted it.
    pub fn thrower(&self) -> Entity {
        self.thrower
    }
}

#[derive(Resource)]
struct EquipmentAssets {
    frag: Handle<Mesh>,
    frag_mat: Handle<StandardMaterial>,
    monkey: Handle<Mesh>,
    monkey_mat: Handle<StandardMaterial>,
    claymore: Handle<Mesh>,
    claymore_mat: Handle<StandardMaterial>,
}

/// Where each Equipment wall's chalk sits on `map`: its Zone, the centre on a wall's face
/// (x, y, z), and which way that face looks.
pub(crate) fn equipment_walls(map: MapId) -> &'static [(EquipmentWall, ZoneId, Vec3, Vec3)] {
    const Q: f32 = QUARRY_X;
    match map {
        MapId::Greybox => {
            const SPOTS: [(EquipmentWall, ZoneId, Vec3, Vec3); 3] = [
                // On the Dining Room's north wall, east of the Yard Door.
                (EquipmentWall::Frags, ZoneId::DiningRoom, Vec3::new(-3.0, 1.5, -14.2), Vec3::NEG_Z),
                // On the Power Room's north wall, the far side of the Yard's AK-74u.
                (EquipmentWall::Claymore, ZoneId::PowerRoom, Vec3::new(6.0, 1.5, -14.2), Vec3::NEG_Z),
                // On the West Hall wall along the Balcony, a storey up, between the window and the ramp.
                (EquipmentWall::BowieKnife, ZoneId::Balcony, Vec3::new(-13.8, 5.5, -8.0), Vec3::X),
            ];
            &SPOTS
        }
        MapId::Quarry => {
            const SPOTS: [(EquipmentWall, ZoneId, Vec3, Vec3); 3] = [
                // On the face of the terrace, across the pit floor from the Pump House.
                (EquipmentWall::Frags, ZoneId::QuarryFloor, Vec3::new(Q - 1.0, 1.5, 1.8), Vec3::NEG_Z),
                // On the Crusher Shed's back wall.
                (EquipmentWall::Claymore, ZoneId::CrusherShed, Vec3::new(Q - 5.0, STOREY + 1.5, 15.8), Vec3::NEG_Z),
                // Up on the Headframe's deck, on its east wall.
                (EquipmentWall::BowieKnife, ZoneId::Headframe, Vec3::new(Q + 5.8, 3.0 * STOREY + 1.5, 38.0), Vec3::NEG_X),
            ];
            &SPOTS
        }
        MapId::Village => {
            const V: f32 = VILLAGE_X;
            const SPOTS: [(EquipmentWall, ZoneId, Vec3, Vec3); 3] = [
                // On the Market's south wall, behind the cross.
                (EquipmentWall::Frags, ZoneId::Market, Vec3::new(V, 1.5, -21.8), Vec3::Z),
                // On the Mill's front wall, beside the Door in from the street.
                (EquipmentWall::Claymore, ZoneId::Mill, Vec3::new(V - 67.0, 1.5, 6.2), Vec3::Z),
                // On the back of the mausoleum.
                (EquipmentWall::BowieKnife, ZoneId::Graveyard, Vec3::new(V + 56.0, 1.5, 45.7), Vec3::Z),
            ];
            &SPOTS
        }
        MapId::Dam => {
            const D: f32 = DAM_X;
            const SPOTS: [(EquipmentWall, ZoneId, Vec3, Vec3); 3] = [
                // On the Tower Head's west wall, beside its window.
                (EquipmentWall::Frags, ZoneId::TowerHead, Vec3::new(D - 45.8, 2.0 * STOREY + 1.5, 30.0), Vec3::X),
                // In the Spillway Gallery, on its south wall.
                (EquipmentWall::Claymore, ZoneId::SpillwayGallery, Vec3::new(D + 30.0, STOREY + 1.5, -3.8), Vec3::Z),
                // On the Control Room's north wall, beside the mimic panel.
                (EquipmentWall::BowieKnife, ZoneId::ControlRoom, Vec3::new(D - 7.5, STOREY + 1.5, 7.8), Vec3::NEG_Z),
            ];
            &SPOTS
        }
    }
}

/// What the left hand is letting go of.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Handled {
    /// A frag thrown with this much fuse left once it leaves the hand.
    Frag(f32),
    MonkeyBomb,
    Claymore,
}

/// The hand handling Equipment after the key is let go: a throw or a plant playing out. The Weapon
/// can't fire until it's done.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct Handling {
    pub what: Handled,
    /// Seconds since the throw or plant began.
    elapsed: f32,
    /// How far up (0 to 1) the hand already was when a frag's throw began.
    raised: f32,
    /// Whether it's still in the hand.
    in_hand: bool,
}

impl Handling {
    /// A throw or a plant just begun, the hand `raised` of the way up if it was cooking a frag.
    pub fn new(what: Handled, raised: f32) -> Self {
        Self { what, elapsed: 0.0, raised, in_hand: true }
    }

    /// Seconds the whole throw or plant takes.
    pub fn secs(&self) -> f32 {
        match self.what {
            Handled::Claymore => PLANT_SECS,
            _ => THROW_SECS,
        }
    }

    /// How far (0 to 1) through it the hand is.
    pub fn progress(&self) -> f32 {
        (self.elapsed / self.secs()).min(1.0)
    }

    fn release_at(&self) -> f32 {
        match self.what {
            Handled::Claymore => PLANT_RELEASE_AT,
            _ => THROW_RELEASE_AT,
        }
    }

    /// Advances it by `dt`. True on the frame the hand lets go.
    pub fn advance(&mut self, dt: f32) -> bool {
        self.elapsed += dt;
        let let_go = self.in_hand && self.progress() >= self.release_at();
        self.in_hand &= !let_go;
        let_go
    }

    pub fn finished(&self) -> bool {
        self.elapsed >= self.secs()
    }

    /// How far the Weapon dips and where the hand is (camera space) with what it holds, if it's in
    /// sight.
    pub fn pose(&self) -> (f32, Option<(Handheld, Transform)>) {
        let (dip, hand) = match self.what {
            Handled::Claymore => plant_pose(self.progress()),
            _ => throw_pose(self.progress(), self.raised),
        };
        let held = match (self.in_hand, self.what) {
            (false, _) => Handheld::Empty,
            (true, Handled::Frag(_)) => Handheld::Frag,
            (true, Handled::MonkeyBomb) => Handheld::MonkeyBomb,
            (true, Handled::Claymore) => Handheld::Claymore,
        };
        (dip, hand.map(|hand| (held, hand)))
    }

    /// Where the hand is (camera space) as it lets go.
    pub fn release_point(&self) -> Vec3 {
        throw_pose(THROW_RELEASE_AT, self.raised).1.map_or(Vec3::ZERO, |hand| hand.translation)
    }
}

/// The left hand as the view model shows it: how far the Weapon dips, and the hand with what it
/// holds, if it's in sight. A cooking frag is held up; otherwise a throw or plant plays out.
pub fn hand_pose(equipment: &Equipment, handling: Option<&Handling>) -> (f32, Option<(Handheld, Transform)>) {
    match (equipment.cooking, handling) {
        (Some(cooked), _) => {
            let (dip, hand) = cook_pose(cook_raise(cooked));
            (dip, Some((Handheld::Frag, hand)))
        }
        (None, Some(handling)) => handling.pose(),
        (None, None) => (0.0, None),
    }
}

/// Run condition: the Player's hand isn't busy with Equipment, so the Weapon can fire.
pub fn arms_free(player: Query<(&Equipment, Has<Handling>), With<Player>>) -> bool {
    player.iter().all(|(equipment, handling)| equipment.cooking.is_none() && !handling)
}

pub struct EquipmentPlugin;

impl Plugin for EquipmentPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<LethalThrown>()
            .add_systems(Startup, load_assets)
            .add_systems(OnEnter(GameState::Playing), spawn_walls)
            .add_systems(
                Update,
                (
                    resolve_frag_limits,
                    restock,
                    (buy_walls.in_set(HostSimulation), price_walls).chain(),
                    (
                        // A paused Player's cook waits in the hand. Starting a throw or a plant
                        // counts as firing, so it waits out a prone transition like one; one
                        // already under way carries on to the end.
                        throw_frags.run_if(input_runs).run_if(not_changing_prone),
                        throw_monkey_bombs.run_if(not_changing_prone),
                        plant_claymores.run_if(not_changing_prone),
                        release_handled,
                        launch_equipment.in_set(HostSimulation),
                    ),
                    (burn_frags, land_monkey_bombs, lure_zombies, trigger_claymores).in_set(HostSimulation),
                )
                    .chain()
                    .run_if(world_runs),
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
    map: Res<MapId>,
) {
    let chalk = materials.add(StandardMaterial {
        base_color: Color::srgb(0.9, 0.9, 0.85),
        perceptual_roughness: 1.0,
        ..default()
    });
    for &(wall, zone, centre, normal) in equipment_walls(*map) {
        let size = match wall {
            EquipmentWall::Frags => Vec3::new(0.3, 0.4, 0.02),
            EquipmentWall::Claymore => Vec3::new(0.5, 0.3, 0.02),
            EquipmentWall::BowieKnife => Vec3::new(0.7, 0.1, 0.02),
        };
        commands.spawn((
            InRun,
            wall,
            Chalk(zone),
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

/// The frag limits follow the Player's modifier entries.
fn resolve_frag_limits(mut player: Single<(&mut Equipment, &Modifiers), With<LocalPlayer>>) {
    let (ref mut equipment, modifiers) = *player;
    let limits = FragLimits::resolve(&modifiers.0);
    if equipment.frag_limits != limits {
        equipment.frag_limits = limits;
    }
}

/// Each new Round restocks the Player's Equipment.
fn restock(
    mut started: MessageReader<RoundStarted>,
    round: Option<Res<Round>>,
    mut equipment: Single<&mut Equipment, With<LocalPlayer>>,
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
    player: Single<(&Equipment, &Knife), With<LocalPlayer>>,
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
    mut players: Query<(&mut Equipment, &mut Knife), With<Player>>,
) {
    for &Purchased { player, buyable } in purchases.read() {
        if let Ok(wall) = walls.get(buyable)
            && let Ok((mut equipment, mut knife)) = players.get_mut(player)
        {
            buy_equipment(*wall, &mut equipment, &mut knife.bowie);
        }
    }
}

/// A kind of Lethal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lethal {
    Frag,
}

/// A Lethal left a Player's hand at `at`; `projectile` is it in flight.
#[derive(Message, Debug, Clone, Copy)]
pub struct LethalThrown {
    pub player: Entity,
    #[allow(dead_code, reason = "no listener tells Lethals apart yet")]
    pub kind: Lethal,
    pub at: Vec3,
    pub projectile: Entity,
}

/// A thrown frag or Monkey Bomb: it bounces off the world and passes through everyone.
fn thrown(mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>, from: Vec3, forward: Vec3) -> impl Bundle {
    (
        InRun,
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material.clone()),
        Transform::from_translation(from),
        RigidBody::Dynamic,
        Collider::sphere(THROWN_RADIUS),
        CollisionLayers::new(GameLayer::Thrown, GameLayer::World),
        Restitution::new(THROWN_RESTITUTION),
        Friction::new(0.8),
        AngularDamping(2.0),
        LinearVelocity(forward * EQUIPMENT_THROW_SPEED + Vec3::Y * EQUIPMENT_THROW_LIFT),
        TranslationInterpolation,
    )
}

/// An explosive Equipment's blast after the Player's stat entries: its damage, both ends of the
/// falloff, and its radius.
pub fn equipment_splash(base: Splash, player: &[Modifier]) -> Splash {
    let damage = resolve(Stat::EquipmentDamage, 1.0, player);
    Splash {
        max: base.max * damage,
        min: base.min * damage,
        radius: resolve(Stat::EquipmentRadius, base.radius, player),
    }
}

/// The Lethal key pulls a frag's pin. Holding it cooks it, and letting go starts the throw with
/// what's left of the fuse. Held too long, it goes off in the hand.
fn throw_frags(
    mut commands: Commands,
    player: Single<(Entity, &mut Equipment, &Modifiers, Has<Handling>), With<LocalPlayer>>,
    camera: Single<&GlobalTransform, With<PlayerCamera>>,
    controls: Controls,
    mut explosions: MessageWriter<Explosion>,
    time: Res<Time>,
) {
    let (entity, mut equipment, modifiers, handling) = player.into_inner();
    let entries = &modifiers.0;
    let held = controls.pressed(Action::Lethal);
    let Some(cooked) = equipment.cooking else {
        if controls.just_pressed(Action::Lethal) && equipment.frags > 0 && !handling {
            equipment.frags -= 1;
            equipment.cooking = Some(0.0);
        }
        return;
    };
    match cook(cooked, time.delta_secs(), held) {
        Cook::Held(cooked) => equipment.cooking = Some(cooked),
        Cook::Thrown(fuse) => {
            equipment.cooking = None;
            // It keeps burning in the hand through the wind-up.
            let fuse = (fuse - THROW_SECS * THROW_RELEASE_AT).max(FRAG_MIN_FUSE_SECS);
            let raised = cook_raise(cooked + time.delta_secs());
            commands.entity(entity).insert(Handling::new(Handled::Frag(fuse), raised));
        }
        Cook::InHand => {
            equipment.cooking = None;
            explosions.write(Explosion {
                at: camera.translation(),
                thrower: Some(entity),
                splash: equipment_splash(FRAG_SPLASH, entries),
                hurts_player: true,
                ammo_mods: AmmoMods::NONE,
                crawlers: makes_crawlers(Explosive::Frag),
            });
        }
    }
}

/// The Monkey Bomb key starts throwing one.
fn throw_monkey_bombs(
    mut commands: Commands,
    player: Single<(Entity, &mut Equipment, Has<Handling>), With<LocalPlayer>>,
    controls: Controls,
) {
    let (entity, mut equipment, handling) = player.into_inner();
    if !controls.just_pressed(Action::MonkeyBomb) || equipment.monkey_bombs == 0 || handling || equipment.cooking.is_some() {
        return;
    }
    equipment.monkey_bombs -= 1;
    commands.entity(entity).insert(Handling::new(Handled::MonkeyBomb, 0.0));
}

/// The Claymore key starts planting one, if there's a floor to plant it on.
fn plant_claymores(
    mut commands: Commands,
    player: Single<(Entity, &mut Equipment, &Transform, Has<Handling>), With<LocalPlayer>>,
    controls: Controls,
    spatial: SpatialQuery,
) {
    let (entity, mut equipment, transform, handling) = player.into_inner();
    if !controls.just_pressed(Action::Claymore) || equipment.claymores == 0 || handling || equipment.cooking.is_some() {
        return;
    }
    if floor_below(transform.translation, &spatial).is_none() {
        return;
    }
    equipment.claymores -= 1;
    commands.entity(entity).insert(Handling::new(Handled::Claymore, 0.0));
}

/// How far the floor is below `from`, if it's near.
fn floor_below(from: Vec3, spatial: &SpatialQuery) -> Option<f32> {
    spatial.cast_ray(from, Dir3::NEG_Y, 3.0, true, &world_filter()).map(|floor| floor.distance)
}

/// Plays out each throw or plant: the frag, Monkey Bomb or Claymore leaves the hand when it lets
/// go, as a `Launch`. A thrown one leaves from where the hand is and flies to the crosshair; a
/// Claymore goes on the floor at the Player's feet, facing where they look, and is given back if
/// there's none by then.
fn release_handled(
    mut commands: Commands,
    player: Option<Single<(Entity, &mut Equipment, &mut Handling, &Transform), With<LocalPlayer>>>,
    camera: Single<&GlobalTransform, With<PlayerCamera>>,
    spatial: SpatialQuery,
    mut launches: MessageWriter<Launch>,
    time: Res<Time>,
) {
    let Some(player) = player else {
        return;
    };
    let (entity, mut equipment, mut handling, transform) = player.into_inner();
    if handling.advance(time.delta_secs()) {
        let from = camera.transform_point(handling.release_point());
        let aim = (camera.translation() + *camera.forward() * THROW_AIM_DISTANCE - from).normalize_or(*camera.forward());
        let thrown = match handling.what {
            Handled::Frag(fuse) => Thrown::Frag { from, aim, fuse },
            Handled::MonkeyBomb => Thrown::MonkeyBomb { from, aim },
            Handled::Claymore => {
                let Some(floor) = floor_below(transform.translation, &spatial) else {
                    equipment.claymores += 1;
                    commands.entity(entity).remove::<Handling>();
                    return;
                };
                let facing = transform.forward().with_y(0.0).normalize_or(Vec3::NEG_Z);
                let at = transform.translation - Vec3::Y * floor + facing * 0.5 + Vec3::Y * 0.08;
                Thrown::Claymore { at, facing }
            }
        };
        launches.write(Launch { player: entity, thrown });
    }
    if handling.finished() {
        commands.entity(entity).remove::<Handling>();
    }
}

/// Makes what a `Launch` threw or planted: a frag or Monkey Bomb in flight, or a Claymore on the
/// floor, each remembering who it's from.
fn launch_equipment(
    mut commands: Commands,
    mut launches: MessageReader<Launch>,
    assets: Res<EquipmentAssets>,
    mut lethals: MessageWriter<LethalThrown>,
) {
    for launch in launches.read() {
        let thrower = launch.player;
        match launch.thrown {
            Thrown::Frag { from, aim, fuse } => {
                let projectile = commands.spawn((Frag { thrower, fuse }, thrown(&assets.frag, &assets.frag_mat, from, aim))).id();
                lethals.write(LethalThrown {
                    player: thrower,
                    kind: Lethal::Frag,
                    at: from,
                    projectile,
                });
            }
            Thrown::MonkeyBomb { from, aim } => {
                commands.spawn((
                    MonkeyBomb { thrower, fuse: None, age: 0.0 },
                    thrown(&assets.monkey, &assets.monkey_mat, from, aim),
                ));
            }
            Thrown::Claymore { at, facing } => {
                commands.spawn((
                    InRun,
                    Claymore { thrower, facing, countdown: None },
                    Mesh3d(assets.claymore.clone()),
                    MeshMaterial3d(assets.claymore_mat.clone()),
                    Transform::from_translation(at).looking_to(facing, Vec3::Y),
                ));
            }
            Thrown::LauncherRound(_) | Thrown::Wonder(_) => {}
        }
    }
}

fn burn_frags(
    mut commands: Commands,
    mut frags: Query<(Entity, &mut Frag, &Transform)>,
    players: Query<&Modifiers, With<Player>>,
    mut explosions: MessageWriter<Explosion>,
    time: Res<Time>,
) {
    for (entity, mut frag, transform) in &mut frags {
        frag.fuse -= time.delta_secs();
        if frag.fuse <= 0.0 {
            let entries = players.get(frag.thrower).map_or(&[][..], |modifiers| &modifiers.0);
            explosions.write(Explosion {
                at: transform.translation + Vec3::Y * 0.1,
                thrower: Some(frag.thrower),
                splash: equipment_splash(FRAG_SPLASH, entries),
                hurts_player: true,
                ammo_mods: AmmoMods::NONE,
                crawlers: makes_crawlers(Explosive::Frag),
            });
            commands.entity(entity).despawn();
        }
    }
}

/// A Monkey Bomb's fuse starts when it lands, and it goes off when the fuse runs out.
fn land_monkey_bombs(
    mut commands: Commands,
    mut bombs: Query<(Entity, &mut MonkeyBomb, &Transform)>,
    players: Query<&Modifiers, With<Player>>,
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
                let entries = players.get(bomb.thrower).map_or(&[][..], |modifiers| &modifiers.0);
                explosions.write(Explosion {
                    at: transform.translation + Vec3::Y * 0.1,
                    thrower: Some(bomb.thrower),
                    splash: equipment_splash(MONKEY_BOMB_SPLASH, entries),
                    hurts_player: true,
                    ammo_mods: AmmoMods::NONE,
                    crawlers: makes_crawlers(Explosive::MonkeyBomb),
                });
                commands.entity(entity).despawn();
            }
            Some(left) => bomb.fuse = Some(left - dt),
        }
    }
}

/// Every landed Monkey Bomb (and every other Pull) draws the Zombies near it, the first to land getting
/// first pick. The Sapper ignores them.
fn lure_zombies(
    mut commands: Commands,
    bombs: Query<(&MonkeyBomb, &Transform)>,
    pulls: Query<&Pull>,
    zombies: Query<(Entity, &Transform, Option<&Lured>, Option<&Entering>), (With<Zombie>, Without<Dying>, Without<Sapper>)>,
) {
    let mut active: Vec<_> = bombs
        .iter()
        .filter_map(|(bomb, transform)| bomb.fuse.map(|fuse| (fuse, transform.translation)))
        .chain(pulls.iter().map(|pull| (pull.left, pull.at)))
        .collect();
    // The least fuse left landed first.
    active.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut targets: Vec<(Entity, Vec3)> = Vec::new();
    for (_, at) in active {
        let candidates = zombies
            .iter()
            .filter(|(entity, ..)| !targets.iter().any(|(taken, _)| taken == entity))
            .map(|(entity, transform, _, entering)| LureCandidate {
                zombie: entity,
                distance: transform.translation.distance(at),
                at_barrier: entering.is_some_and(Entering::at_barrier),
            });
        targets.extend(lured(candidates).into_iter().map(|zombie| (zombie, at)));
    }

    for (entity, _, lure, _) in &zombies {
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
    players: Query<&Modifiers, With<Player>>,
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
                let entries = players.get(claymore.thrower).map_or(&[][..], |modifiers| &modifiers.0);
                explosions.write(Explosion {
                    at: at + Vec3::Y * 0.3,
                    thrower: Some(claymore.thrower),
                    splash: equipment_splash(CLAYMORE_SPLASH, entries),
                    hurts_player: false,
                    ammo_mods: AmmoMods::NONE,
                    crawlers: makes_crawlers(Explosive::Claymore),
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
    fn a_throw_lets_go_once_partway_through_and_then_finishes() {
        let mut handling = Handling::new(Handled::MonkeyBomb, 0.0);
        let mut lets_go = 0;
        let mut elapsed = 0.0;
        while !handling.finished() {
            let before = handling.in_hand;
            if handling.advance(0.01) {
                lets_go += 1;
                assert!(before && !handling.in_hand);
                assert!((elapsed / THROW_SECS - THROW_RELEASE_AT).abs() < 0.03, "at {elapsed}");
            }
            elapsed += 0.01;
        }
        assert_eq!(lets_go, 1);
        assert!((THROW_SECS..THROW_SECS + 0.02).contains(&elapsed));
    }

    #[test]
    fn a_plant_takes_longer_than_a_throw() {
        assert!(Handling::new(Handled::Claymore, 0.0).secs() > Handling::new(Handled::Frag(1.0), 0.0).secs());
    }

    #[test]
    fn the_hand_holds_the_equipment_until_it_lets_go() {
        let mut handling = Handling::new(Handled::Frag(2.0), 1.0);
        assert_eq!(handling.pose().1.map(|(held, _)| held), Some(Handheld::Frag));
        handling.advance(THROW_SECS * (THROW_RELEASE_AT + 0.01));
        assert_eq!(handling.pose().1.map(|(held, _)| held), Some(Handheld::Empty));
        handling.advance(THROW_SECS);
        assert!(handling.pose().1.is_none(), "out of sight once it's done");
    }

    #[test]
    fn a_throw_leaves_from_the_hand_in_front_of_the_camera() {
        let release = Handling::new(Handled::MonkeyBomb, 0.0).release_point();
        assert!(release.z < -0.1 && release.z > -0.7, "{release}");
    }

    #[test]
    fn a_cooking_frag_is_held_up_and_the_hand_is_otherwise_idle() {
        let idle = Equipment::default();
        assert_eq!(hand_pose(&idle, None), (0.0, None));
        let cooking = Equipment { cooking: Some(1.0), ..Equipment::default() };
        let (dip, hand) = hand_pose(&cooking, None);
        assert!(dip > 0.99 && hand.map(|(held, _)| held) == Some(Handheld::Frag));
        let throwing = Handling::new(Handled::Frag(1.0), 1.0);
        assert_eq!(hand_pose(&idle, Some(&throwing)).0, throwing.pose().0);
    }

    #[test]
    fn equipment_entries_raise_blast_damage_and_radius() {
        assert_eq!(equipment_splash(FRAG_SPLASH, &[]), FRAG_SPLASH);
        let source = crate::modifier::Source("test");
        let entries = [
            Modifier { stat: Stat::EquipmentDamage, source, effect: crate::modifier::Effect::Mult(1.25) },
            Modifier { stat: Stat::EquipmentRadius, source, effect: crate::modifier::Effect::Mult(1.1) },
        ];
        let splash = equipment_splash(CLAYMORE_SPLASH, &entries);
        assert!((splash.max - CLAYMORE_SPLASH.max * 1.25).abs() < 1e-3);
        assert!((splash.min - CLAYMORE_SPLASH.min * 1.25).abs() < 1e-3);
        assert!((splash.radius - CLAYMORE_SPLASH.radius * 1.1).abs() < 1e-4);
    }

    #[test]
    fn the_player_starts_with_two_frags_and_nothing_else() {
        let equipment = Equipment::default();
        assert_eq!(equipment.frags, 2);
        assert_eq!((equipment.claymores, equipment.monkey_bombs), (0, 0));
    }

    #[test]
    fn frags_restock_by_two_a_round_up_to_four() {
        let classic = FragLimits::default();
        assert_eq!(frags_after_round(0, classic), 2);
        assert_eq!(frags_after_round(1, classic), 3);
        assert_eq!(frags_after_round(2, classic), 4);
        assert_eq!(frags_after_round(3, classic), 4);
        assert_eq!(frags_after_round(4, classic), 4);
    }

    #[test]
    fn lethal_entries_raise_the_most_carried_and_the_restock() {
        use crate::modifier::{Effect, Source};
        let entries = [Stat::LethalCapacity, Stat::LethalRestock].map(|stat| Modifier {
            stat,
            source: Source("A"),
            effect: Effect::Flat(1.0),
        });
        let limits = FragLimits::resolve(&entries);
        assert_eq!(limits, FragLimits { max: 5, per_round: 3 });
        assert_eq!(frags_after_round(0, limits), 3);
        assert_eq!(frags_after_round(4, limits), 5);
        let mut equipment = Equipment { frags: 0, frag_limits: limits, ..Equipment::default() };
        let mut bowie = false;
        buy_equipment(EquipmentWall::Frags, &mut equipment, &mut bowie);
        assert_eq!(equipment.frags, 5);
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
            frag_limits: FragLimits::default(),
            host_dirty: false,
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
        for &(wall, zone, at, _) in MapId::ALL.iter().flat_map(|&map| equipment_walls(map)) {
            let (lo, hi) = zone.volume();
            let inside = (lo.x..=hi.x).contains(&at.x) && (lo.y..=hi.y).contains(&at.y) && (lo.z..=hi.z).contains(&at.z);
            assert!(inside, "{wall:?} is outside {}", zone.name());
        }
    }
}
