//! Teammates on screen: every Player but the `LocalPlayer` is drawn as a slim blocky body, built
//! like the Zombie's (boxes on pivots, posed in code every frame), with a torso in their Seat's
//! colour, their held Weapon at its world scale, and a name tag over their head.
//!
//! What the body does comes from `Pose`, a few bits of state about the Player, and from `Shots`
//! and `HeldWeapon`. On the Host and in a two-Player world `fill_poses` fills them from the
//! Player's own state; on a client they will come off snapshots instead. The body reads only
//! those, so it draws the same either way. Purely visual: nothing here is in `HostSimulation`.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::ammo_mod::{AmmoMods, glint_colour};
use crate::arsenal::{WeaponClass, WeaponId};
use crate::attachment::Attachment;
use crate::equipment::Handling;
use crate::gun_model::{GunModels, Handheld, WORLD_SCALE, model_length};
use crate::last_stand::LastStand;
use crate::layers::GameLayer;
use crate::muzzle::{aim_direction, muzzle_position};
use crate::perk::{Drinking, Perk};
use crate::player::{LocalPlayer, Locomotion, MoveInput, Player, PlayerCamera, PlayerName, Seat};
use crate::state::{InRun, world_runs};
use crate::tuning::*;
use crate::weapon::{Knife, Loadout, UPGRADED_PURPLE, WeaponCopy, glint_of};

/// How a Player stands. Crouching and prone join it with Crouch and prone (#140); the two bits it
/// takes on the wire are already set aside for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Stance {
    #[default]
    Standing,
}

/// The one-off thing a Player's hands are doing, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Action {
    #[default]
    None,
    Reload,
    Knife,
    Throw,
    Drink,
    /// Kneeling to Revive a teammate. Nothing fills it until Co-op's Revive exists.
    #[allow(dead_code, reason = "filled by Revive")]
    Revive,
}

/// What a Player's body is doing, as far as a teammate can see. It packs into 2 bytes (see
/// `pack`), which is what a snapshot carries. Running isn't in it: it's read off how the Player
/// moves. Each shot is in `Shots`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pose {
    pub stance: Stance,
    pub sprinting: bool,
    /// Aiming down the sights.
    pub aiming: bool,
    pub airborne: bool,
    pub last_stand: bool,
    pub action: Action,
}

impl Pose {
    /// The two bytes a snapshot carries: stance in bits 0-1, then sprinting, aiming, airborne and
    /// Last Stand, then the action in bits 6-8.
    pub fn pack(self) -> u16 {
        let stance = match self.stance {
            Stance::Standing => 0,
        };
        let action = match self.action {
            Action::None => 0,
            Action::Reload => 1,
            Action::Knife => 2,
            Action::Throw => 3,
            Action::Drink => 4,
            Action::Revive => 5,
        };
        stance
            | u16::from(self.sprinting) << 2
            | u16::from(self.aiming) << 3
            | u16::from(self.airborne) << 4
            | u16::from(self.last_stand) << 5
            | action << 6
    }

    /// The Pose in `bits`. Anything it doesn't know is read as the plain case.
    pub fn unpack(bits: u16) -> Self {
        Self {
            stance: Stance::Standing,
            sprinting: bits & (1 << 2) != 0,
            aiming: bits & (1 << 3) != 0,
            airborne: bits & (1 << 4) != 0,
            last_stand: bits & (1 << 5) != 0,
            action: match bits >> 6 & 0b111 {
                1 => Action::Reload,
                2 => Action::Knife,
                3 => Action::Throw,
                4 => Action::Drink,
                5 => Action::Revive,
                _ => Action::None,
            },
        }
    }
}

/// Shots a Player has fired, wrapping at 256. Each step is one shot for their teammates to show.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct Shots {
    count: u8,
    /// How long the Loadout said it was since the last shot, to spot the next one.
    since: f32,
}

impl Shots {
    pub fn count(&self) -> u8 {
        self.count
    }

    /// Sets the count to what the network says it is.
    pub fn set(&mut self, count: u8) {
        self.count = count;
    }
}

/// A Player whose `Pose`, `Shots` and `HeldWeapon` come over the network, not from their own state:
/// a remote Player on the Host, and every puppet on a client.
#[derive(Component)]
pub struct Told;

/// The Weapon a Player holds, as a teammate sees it.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct HeldWeapon {
    pub id: WeaponId,
    /// What colours its glint, if it's Upgraded.
    pub glint: Option<AmmoMods>,
    pub attachments: Vec<Attachment>,
}

impl HeldWeapon {
    pub fn of(copy: &WeaponCopy) -> Self {
        Self { id: copy.id, glint: glint_of(copy), attachments: copy.attachments.clone() }
    }
}

impl Default for HeldWeapon {
    fn default() -> Self {
        Self { id: STARTING_WEAPON, glint: None, attachments: Vec::new() }
    }
}

/// A teammate's gun went off: where its muzzle is and which way they aim, for `muzzle` to draw a
/// flash on the gun and, on some shots, a tracer.
#[derive(Message, Debug, Clone, Copy)]
pub struct TeammateShot {
    pub gun: Entity,
    pub class: WeaponClass,
    pub muzzle: Vec3,
    pub aim: Vec3,
    pub tracer: bool,
}

/// The pivot a teammate's Weapon hangs from, where their muzzle flash is attached.
#[derive(Component)]
pub struct TeammateGun;

/// A Player waiting to Respawn: they have no body and no name tag. Nothing adds it until
/// Respawning exists (#155).
#[derive(Component)]
#[allow(dead_code, reason = "inserted by Respawning (#155)")]
pub struct AwaitingRespawn;

/// Fills each Player's `Pose`, `Shots` and `HeldWeapon` from their own state.
fn fill_poses(
    mut players: Query<
        (
            &Locomotion,
            &Loadout,
            &Knife,
            Has<Handling>,
            Has<Drinking>,
            Has<LastStand>,
            &mut Pose,
            &mut Shots,
            &mut HeldWeapon,
        ),
        (With<Player>, Without<Told>),
    >,
) {
    for (locomotion, loadout, knife, throwing, drinking, last_stand, mut pose, mut shots, mut held) in &mut players {
        let action = if drinking {
            Action::Drink
        } else if throwing {
            Action::Throw
        } else if knife.swing().is_some() {
            Action::Knife
        } else if loadout.reload.is_some() {
            Action::Reload
        } else {
            Action::None
        };
        let filled = Pose {
            stance: Stance::Standing,
            sprinting: locomotion.sprinting,
            aiming: loadout.aim > 0.5,
            airborne: !locomotion.grounded(),
            last_stand,
            action,
        };
        pose.set_if_neq(filled);

        let since = loadout.since_shot();
        if since < shots.since {
            shots.count = shots.count.wrapping_add(1);
        }
        shots.since = since;

        let now = HeldWeapon::of(loadout.held());
        if *held != now {
            *held = now;
        }
    }
}

// The body, in metres, standing on the floor. The Player's capsule is 1.8 m tall.
const LEG: Vec3 = Vec3::new(0.17, 0.78, 0.17);
const TORSO: Vec3 = Vec3::new(0.4, 0.6, 0.22);
const ARM: Vec3 = Vec3::new(0.13, 0.52, 0.13);
const HEAD: f32 = 0.27;
const SHOULDER_X: f32 = (TORSO.x + ARM.x) / 2.0;
const SHOULDER_Y: f32 = TORSO.y - 0.06;
const LEG_X: f32 = 0.1;
/// How quickly the body catches up with the pose it's heading for. Higher is snappier.
const FOLLOW_RATE: f32 = 16.0;
/// The fastest a Player is taken to move, so a teleport doesn't spin their legs.
const MAX_GAIT_SPEED: f32 = 12.0;
/// How long a shot's kick takes to settle.
const KICK_SECS: f32 = 0.18;
/// Where the Weapon rests against the shoulder pivot, held low and ready.
const GUN_READY: Vec3 = Vec3::new(0.1, -0.2, -0.42);
/// A name tag's width in pixels; the name is centred in it.
const NAME_TAG_WIDTH: f32 = 200.0;

/// One arm's turn from hanging straight down: `pitch` swings it forward, `yaw` across the body.
/// Written for the right arm; the left mirrors it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Arm {
    pitch: f32,
    yaw: f32,
}

impl Arm {
    fn rotation(self, side: usize) -> Quat {
        let mirror = if side == 0 { -1.0 } else { 1.0 };
        Quat::from_rotation_y(self.yaw * mirror) * Quat::from_rotation_x(self.pitch)
    }
}

/// What a hand holds besides the Weapon.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Prop {
    None,
    Knife,
    Frag,
    /// A hand with nothing in it, after a throw lets go.
    Empty,
    Bottle,
}

/// What a body is doing this frame.
struct Doing {
    pose: Pose,
    /// Horizontal speed in m/s, and how far through its walk cycle the legs are.
    speed: f32,
    phase: f32,
    /// How fresh the last shot is, 1 (just fired) to 0.
    kick: f32,
    /// Seconds since the action began, and how far (0 to 1) through its length that is.
    clock: f32,
    progress: f32,
}

/// Where every pivot of a body should head, before it's smoothed.
struct Body {
    /// The hips' height above the floor, and how far the whole body tips over from upright,
    /// face down (a Last Stand crawl).
    hips_y: f32,
    tilt: f32,
    /// The upper body's lean forward from the hips.
    lean: f32,
    /// The head's nod, up positive, beyond following the Player's look.
    nod: f32,
    /// The Weapon's and arms' pitch beyond following the Player's look.
    aim: f32,
    arms: [Arm; 2],
    legs: [f32; 2],
    /// Where the Weapon sits against the shoulders, and how it rolls.
    gun: Vec3,
    gun_roll: f32,
    gun_shown: bool,
    prop: Prop,
}

/// Sets whether something shows, touching it only if that changes.
fn show(visibility: &mut Visibility, shown: bool) {
    let to = if shown { Visibility::Inherited } else { Visibility::Hidden };
    if *visibility != to {
        *visibility = to;
    }
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// 0 before `from`, 1 after `to`, easing between.
fn between(t: f32, from: f32, to: f32) -> f32 {
    smoothstep((t - from) / (to - from))
}

/// The body for what a Player is doing.
fn body(doing: &Doing) -> Body {
    let Doing { pose, speed, phase, kick, progress, clock } = *doing;
    let stride = (speed / 1.5).min(1.0);
    let run = ((speed - 3.0) / 3.0).clamp(0.0, 1.0);
    let swing = (0.3 + 0.3 * run) * stride;
    let mut body = Body {
        hips_y: LEG.y - 0.03 * stride * (1.0 - (2.0 * phase).cos()),
        tilt: 0.0,
        lean: 0.05 + 0.1 * run + 0.04 * kick,
        nod: 0.0,
        aim: 0.06 * kick,
        // Holding the Weapon low and ready: the right hand at the grip, the left under the fore-end.
        arms: [Arm { pitch: 1.1, yaw: 0.5 }, Arm { pitch: 1.3, yaw: 0.5 }],
        legs: [swing * phase.sin(), -swing * phase.sin()],
        gun: GUN_READY + Vec3::new(0.0, 0.01 * stride * (2.0 * phase).sin(), 0.05 * kick),
        gun_roll: 0.0,
        gun_shown: true,
        prop: Prop::None,
    };

    if pose.aiming {
        // The Weapon comes up to the eye, both hands on it.
        body.gun = Vec3::new(0.0, -0.02, -0.4 + 0.04 * kick);
        body.arms = [Arm { pitch: 1.35, yaw: 0.6 }, Arm { pitch: 1.5, yaw: 0.5 }];
        body.lean = 0.12;
    }
    if pose.sprinting {
        // The Weapon carried down and across, the legs driving.
        let drive = 0.8 * stride.max(0.5) * phase.sin();
        body.lean = 0.3;
        body.aim = -0.7;
        body.gun = Vec3::new(0.05, -0.28, -0.3);
        body.arms = [Arm { pitch: 0.7, yaw: 0.3 }, Arm { pitch: 0.9, yaw: 0.4 }];
        body.legs = [drive, -drive];
    }
    if pose.airborne && !pose.last_stand {
        // Legs tucked, arms thrown out a little.
        body.legs = [0.7, -0.35];
        body.arms[0].pitch += 0.3;
        body.arms[1].pitch += 0.3;
    }
    match pose.action {
        Action::None => {}
        Action::Reload => {
            // The Weapon cants over while the left hand drops to the belt for a magazine and
            // brings it back up.
            let fetch = between(progress, 0.1, 0.35) * (1.0 - between(progress, 0.6, 0.85));
            body.gun_roll = 0.5 * between(progress, 0.05, 0.2) * (1.0 - between(progress, 0.9, 1.0));
            body.arms[0] = Arm { pitch: 1.1 - 0.9 * fetch, yaw: 0.5 - 0.3 * fetch };
            body.gun.y += 0.08 * fetch;
        }
        Action::Knife => {
            // The right hand lifts the blade up and back, then slashes it down across the body.
            let wind = between(progress, 0.0, 0.3);
            let slash = between(progress, 0.3, 0.6);
            let back = between(progress, 0.75, 1.0);
            let raised = 2.3 * wind;
            let yawed = -0.2 * wind;
            body.arms[1] = Arm {
                pitch: (raised + (0.6 - raised) * slash).lerp(1.1, back),
                yaw: (yawed + (0.9 - yawed) * slash).lerp(0.25, back),
            };
            body.arms[0] = Arm { pitch: 0.5, yaw: 0.2 };
            body.lean += 0.25 * slash * (1.0 - back);
            body.gun_shown = false;
            body.prop = Prop::Knife;
        }
        Action::Throw => {
            // The right arm winds back then throws overhand, letting go at the release point.
            let wind = between(progress, 0.0, THROW_RELEASE_AT * 0.6);
            let throw = between(progress, THROW_RELEASE_AT * 0.6, THROW_RELEASE_AT);
            let settle = between(progress, 0.7, 1.0);
            let pitch = 2.6 * wind + (1.2 - 2.6) * throw;
            body.arms[1] = Arm { pitch: pitch.lerp(1.1, settle), yaw: 0.2 };
            body.arms[0] = Arm { pitch: 0.6, yaw: 0.3 };
            body.lean += 0.2 * throw * (1.0 - settle);
            body.gun_shown = false;
            body.prop = if progress < THROW_RELEASE_AT { Prop::Frag } else { Prop::Empty };
        }
        Action::Drink => {
            // A bottle raised to the mouth in the right hand, tipped back to drink.
            let lift = between(progress, 0.0, 0.2) * (1.0 - between(progress, 0.85, 1.0));
            body.arms[1] = Arm { pitch: 1.1 + 1.3 * lift, yaw: 0.6 * lift + 0.25 * (1.0 - lift) };
            body.arms[0] = Arm { pitch: 0.4, yaw: 0.2 };
            body.nod = 0.3 * lift;
            body.gun_shown = false;
            body.prop = Prop::Bottle;
        }
        Action::Revive => {
            // Down on one knee, both hands out in front, working.
            body.hips_y = 0.5;
            body.legs = [1.3, -0.5];
            body.lean = 0.25;
            let work = 0.15 * (clock * 6.0).sin();
            body.arms = [Arm { pitch: 0.9 + work, yaw: 0.1 }, Arm { pitch: 0.9 - work, yaw: 0.1 }];
            body.gun_shown = false;
        }
    }
    if pose.last_stand {
        // Face down, hauling themselves along on their arms with their legs dragging.
        let haul = stride * phase.sin();
        body.hips_y = 0.22;
        body.tilt = 1.4;
        body.lean = 0.0;
        body.nod = 1.2;
        body.legs = [0.1 * haul, -0.1 * haul];
        body.arms = [Arm { pitch: PI * 0.8 + 0.4 * haul, yaw: 0.0 }, Arm { pitch: PI * 0.8 - 0.4 * haul, yaw: 0.0 }];
        body.gun = GUN_READY;
        body.gun_roll = 0.0;
        body.aim = 0.0;
        body.gun_shown = matches!(pose.action, Action::None | Action::Reload);
        body.prop = Prop::None;
    }
    body
}

/// The pivots of one teammate's body, posed by `animate`, on the Player they are the body of.
#[derive(Component)]
struct Rig {
    root: Entity,
    hips: Entity,
    torso: Entity,
    head: Entity,
    /// The pivot the arms and Weapon hang from, which follows where the Player looks.
    aim: Entity,
    /// Left, then right.
    arms: [Entity; 2],
    legs: [Entity; 2],
    gun: Entity,
    /// The Weapon's model, then the frame its Attachments and glint hang in.
    gun_model: Entity,
    stretch: Entity,
    /// Its Attachments hang here, replaced when the Weapon changes.
    fitted: Entity,
    glint: Entity,
    glint_material: Handle<StandardMaterial>,
    prop: Entity,
    /// The Weapon that's dressed on it now.
    shown: Option<HeldWeapon>,
    // How the body has been moving.
    phase: f32,
    speed: f32,
    last: Option<Vec3>,
    action: Action,
    action_clock: f32,
    shots: u8,
    kick: f32,
    sweep: f32,
    pass: u32,
    prop_shown: Prop,
}

/// A teammate's name tag, a label over their head.
#[derive(Component)]
struct NameTag(Entity);

fn spawn_bodies(
    mut commands: Commands,
    new: Query<(Entity, &Seat, &PlayerName), (Added<Player>, Without<LocalPlayer>)>,
    guns: Res<GunModels>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (player, seat, name) in &new {
        let colour = seat.colour();
        let linear = colour.to_linear();
        let torso_material = materials.add(colour);
        let arm_material = materials.add(Color::linear_rgb(linear.red * 0.7, linear.green * 0.7, linear.blue * 0.7));
        let leg_material = materials.add(Color::srgb(0.18, 0.19, 0.22));
        let head_material = materials.add(Color::srgb(0.82, 0.62, 0.46));
        let glint_material = materials.add(StandardMaterial {
            base_color: UPGRADED_PURPLE,
            emissive: LinearRgba::from(UPGRADED_PURPLE) * 8.0,
            unlit: true,
            ..default()
        });
        let boxed = |meshes: &mut Assets<Mesh>, size: Vec3| Mesh3d(meshes.add(Cuboid::from_size(size)));
        let half_height = PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0;
        let pivot = |commands: &mut Commands, at: Vec3, parent: Entity| {
            commands.spawn((Transform::from_translation(at), Visibility::default(), ChildOf(parent))).id()
        };

        let root = pivot(&mut commands, Vec3::NEG_Y * half_height, player);
        let hips = pivot(&mut commands, Vec3::Y * LEG.y, root);
        let torso = pivot(&mut commands, Vec3::ZERO, hips);
        commands.spawn((
            boxed(&mut meshes, TORSO),
            MeshMaterial3d(torso_material),
            Transform::from_xyz(0.0, TORSO.y / 2.0, 0.0),
            ChildOf(torso),
        ));
        let head = pivot(&mut commands, Vec3::Y * TORSO.y, torso);
        commands.spawn((
            boxed(&mut meshes, Vec3::splat(HEAD)),
            MeshMaterial3d(head_material),
            Transform::from_xyz(0.0, HEAD / 2.0 + 0.03, 0.0),
            ChildOf(head),
        ));
        let aim = pivot(&mut commands, Vec3::Y * SHOULDER_Y, torso);
        let arms = [0, 1].map(|side| {
            let x = if side == 0 { -SHOULDER_X } else { SHOULDER_X };
            let arm = pivot(&mut commands, Vec3::X * x, aim);
            commands.spawn((
                boxed(&mut meshes, ARM),
                MeshMaterial3d(arm_material.clone()),
                Transform::from_xyz(0.0, -ARM.y / 2.0, 0.0),
                ChildOf(arm),
            ));
            arm
        });
        let legs = [0, 1].map(|side| {
            let x = if side == 0 { -LEG_X } else { LEG_X };
            let leg = pivot(&mut commands, Vec3::X * x, hips);
            commands.spawn((
                boxed(&mut meshes, LEG),
                MeshMaterial3d(leg_material.clone()),
                Transform::from_xyz(0.0, -LEG.y / 2.0, 0.0),
                ChildOf(leg),
            ));
            leg
        });
        // What the right hand holds besides the Weapon, at the end of the arm.
        let prop = commands
            .spawn((
                Mesh3d(guns.knife.clone()),
                MeshMaterial3d(guns.material.clone()),
                Transform::from_xyz(0.0, -ARM.y, 0.0).with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                Visibility::Hidden,
                ChildOf(arms[1]),
            ))
            .id();

        // The Weapon, as it floats over the Mystery Box but with its muzzle forward.
        let gun = pivot(&mut commands, GUN_READY, aim);
        commands.entity(gun).insert(TeammateGun);
        let gun_model = commands
            .spawn((
                Mesh3d(guns.world(STARTING_WEAPON)),
                MeshMaterial3d(guns.material.clone()),
                Transform::from_rotation(Quat::from_rotation_y(-FRAC_PI_2)),
                ChildOf(gun),
            ))
            .id();
        // Attachments and the glint are placed as on the view model, at the world scale.
        let frame = commands
            .spawn((Transform::from_scale(Vec3::splat(WORLD_SCALE)), Visibility::default(), ChildOf(gun)))
            .id();
        let stretch = pivot(&mut commands, Vec3::ZERO, frame);
        let fitted = pivot(&mut commands, Vec3::ZERO, stretch);
        let glint = commands
            .spawn((
                Mesh3d(meshes.add(Cuboid::new(0.066, 0.106, 0.04))),
                MeshMaterial3d(glint_material.clone()),
                Transform::default(),
                Visibility::Hidden,
                ChildOf(stretch),
            ))
            .id();

        commands.entity(player).insert(Rig {
            root,
            hips,
            torso,
            head,
            aim,
            arms,
            legs,
            gun,
            gun_model,
            stretch,
            fitted,
            glint,
            glint_material,
            prop,
            shown: None,
            phase: 0.0,
            speed: 0.0,
            last: None,
            action: Action::None,
            action_clock: 0.0,
            shots: 0,
            kick: 0.0,
            sweep: 0.0,
            pass: 0,
            prop_shown: Prop::None,
        });
        commands.spawn((
            NameTag(player),
            InRun,
            Text::new(name.0.clone()),
            TextFont { font_size: FontSize::Px(NAME_TAG_FONT_PX), ..default() },
            TextColor(colour),
            TextLayout::justify(Justify::Center),
            Node {
                position_type: PositionType::Absolute,
                width: px(NAME_TAG_WIDTH),
                ..default()
            },
            Visibility::Hidden,
        ));
    }
}

/// How long an action takes, for the body to play it out over. `None` for one with no set length.
fn action_secs(action: Action, held: &HeldWeapon) -> Option<f32> {
    match action {
        Action::None | Action::Revive => None,
        Action::Reload => {
            let stats = held.id.stats();
            Some(stats.reload.secs(0, stats.magazine).max(0.5))
        }
        Action::Knife => Some(KNIFE_COOLDOWN_SECS),
        Action::Throw => Some(THROW_SECS),
        Action::Drink => Some(PERK_DRINK_SECS),
    }
}

/// Poses every teammate's body from their `Pose`, and how they move.
fn animate(
    mut players: Query<(&mut Rig, &Pose, &Shots, &HeldWeapon, &MoveInput, &Transform, Has<AwaitingRespawn>)>,
    mut parts: Query<(&mut Transform, &mut Visibility), Without<Rig>>,
    mut meshes: Query<&mut Mesh3d>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    guns: Res<GunModels>,
    time: Res<Time>,
    mut commands: Commands,
    global: Query<&GlobalTransform>,
    mut fired: MessageWriter<TeammateShot>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let follow = 1.0 - (-FOLLOW_RATE * dt).exp();
    for (mut rig, pose, shots, held, input, transform, bodiless) in &mut players {
        if let Ok((_, mut visibility)) = parts.get_mut(rig.root) {
            show(&mut visibility, !bodiless);
        }
        if bodiless {
            continue;
        }

        // How they're moving.
        let at = transform.translation;
        let moved = rig.last.map_or(0.0, |last| Vec2::new(at.x - last.x, at.z - last.z).length());
        rig.last = Some(at);
        let speed = (moved / dt).min(MAX_GAIT_SPEED);
        rig.speed = rig.speed.lerp(speed, 1.0 - (-10.0 * dt).exp());
        let cycle = if pose.last_stand { 0.9 } else { 1.5 + 0.12 * rig.speed };
        rig.phase = (rig.phase + moved.min(MAX_GAIT_SPEED * dt) / cycle * TAU) % TAU;

        // Each step of the shot counter is a shot to kick for.
        if shots.count() != rig.shots {
            rig.shots = shots.count();
            rig.kick = 1.0;
            if let Ok(gun) = global.get(rig.gun) {
                let class = held.id.stats().class;
                fired.write(TeammateShot {
                    gun: rig.gun,
                    class,
                    muzzle: gun.transform_point(muzzle_position(class) * WORLD_SCALE),
                    aim: aim_direction(input.yaw, input.pitch),
                    tracer: u32::from(shots.count()).is_multiple_of(TRACER_EVERY),
                });
            }
        } else {
            rig.kick = (rig.kick - dt / KICK_SECS).max(0.0);
        }

        // One-off actions play once from where they begin.
        if pose.action != rig.action {
            rig.action = pose.action;
            rig.action_clock = 0.0;
        }
        rig.action_clock += dt;
        let secs = action_secs(pose.action, held);
        if let Some(secs) = secs
            && rig.action_clock >= secs
            && matches!(pose.action, Action::Knife | Action::Throw)
        {
            // Another swing or throw straight after the last.
            rig.action_clock -= secs;
        }
        let progress = secs.map_or(0.0, |secs| (rig.action_clock / secs).min(1.0));

        let target = body(&Doing {
            pose: *pose,
            speed: rig.speed,
            phase: rig.phase,
            kick: rig.kick,
            clock: rig.action_clock,
            progress,
        });

        let mut set = |part: Entity, f: &dyn Fn(&mut Transform, &mut Visibility)| {
            if let Ok((mut transform, mut visibility)) = parts.get_mut(part) {
                f(&mut transform, &mut visibility);
            }
        };
        let tilt = Quat::from_rotation_x(-target.tilt);
        set(rig.hips, &|t, _| {
            t.translation.y = t.translation.y.lerp(target.hips_y, follow);
            t.rotation = t.rotation.slerp(tilt, follow);
        });
        let lean = Quat::from_rotation_x(-target.lean);
        set(rig.torso, &|t, _| t.rotation = t.rotation.slerp(lean, follow));
        // The head looks where the Player does.
        let nod = Quat::from_rotation_x(input.pitch * 0.7 + target.nod);
        set(rig.head, &|t, _| t.rotation = t.rotation.slerp(nod, follow));
        let aim = Quat::from_rotation_x(input.pitch * 0.8 + target.aim);
        set(rig.aim, &|t, _| t.rotation = t.rotation.slerp(aim, follow));
        for side in 0..2 {
            let arm = target.arms[side].rotation(side);
            set(rig.arms[side], &|t, _| t.rotation = t.rotation.slerp(arm, follow));
            let leg = Quat::from_rotation_x(target.legs[side]);
            set(rig.legs[side], &|t, _| t.rotation = t.rotation.slerp(leg, follow));
        }
        let roll = Quat::from_rotation_z(target.gun_roll);
        set(rig.gun, &|t, visibility| {
            t.translation = t.translation.lerp(target.gun, follow);
            t.rotation = t.rotation.slerp(roll, follow);
            show(visibility, target.gun_shown);
        });

        // The right hand's prop.
        if rig.prop_shown != target.prop {
            rig.prop_shown = target.prop;
            let shown = match target.prop {
                Prop::None => None,
                Prop::Knife => Some(guns.knife.clone()),
                Prop::Frag => Some(guns.handheld(Handheld::Frag)),
                Prop::Empty => Some(guns.handheld(Handheld::Empty)),
                Prop::Bottle => Some(guns.bottle(Perk::ALL[0])),
            };
            let visible = shown.is_some();
            if let (Some(shown), Ok(mut mesh)) = (shown, meshes.get_mut(rig.prop)) {
                mesh.0 = shown;
            }
            set(rig.prop, &|_, visibility| {
                show(visibility, visible);
            });
        }

        // The Weapon in hand, with its Attachments.
        if rig.shown.as_ref() != Some(held) {
            let length = model_length(held.id.stats().class);
            if let Ok(mut mesh) = meshes.get_mut(rig.gun_model) {
                mesh.0 = guns.world(held.id);
            }
            set(rig.stretch, &|t, _| t.scale = Vec3::new(1.0, 1.0, length));
            commands.entity(rig.fitted).despawn_related::<Children>();
            for &attachment in &held.attachments {
                if let Some(part) = guns.attachment(held.id, attachment) {
                    commands.spawn((
                        Mesh3d(part.mesh.clone()),
                        MeshMaterial3d(guns.material.clone()),
                        part.rest,
                        ChildOf(rig.fitted),
                    ));
                }
            }
            rig.shown = Some(held.clone());
        }
        // The glint sweeps from the back of the model to the muzzle, over and over.
        if let Some(ammo_mods) = held.glint {
            let colour = glint_colour(ammo_mods, rig.pass, UPGRADED_PURPLE);
            if let Some(mut material) = materials.get_mut(&rig.glint_material) {
                material.base_color = colour;
                material.emissive = LinearRgba::from(colour) * 8.0;
            }
            let swept = rig.sweep + dt / GLINT_SWEEP_SECS;
            rig.pass = rig.pass.wrapping_add(swept as u32);
            rig.sweep = swept.fract();
            let z = 0.15 - 0.3 * rig.sweep;
            set(rig.glint, &|t, visibility| {
                t.translation.z = z;
                show(visibility, true);
            });
        } else {
            set(rig.glint, &|_, visibility| {
                show(visibility, false);
            });
        }
    }
}

/// How strong a name tag is at `distance` metres: 1 up close, fading to 0 far away.
fn tag_strength(distance: f32) -> f32 {
    ((NAME_TAG_FADE_END - distance) / (NAME_TAG_FADE_END - NAME_TAG_FADE_START)).clamp(0.0, 1.0)
}

/// Floats each name tag over its teammate's head, in their colour, unless a wall is in the way.
fn place_name_tags(
    mut commands: Commands,
    mut tags: Query<(Entity, &NameTag, &mut Node, &mut Visibility, &mut TextColor)>,
    players: Query<(&GlobalTransform, &Seat, Has<AwaitingRespawn>), With<Player>>,
    camera: Single<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    spatial: SpatialQuery,
) {
    let (camera, camera_at) = *camera;
    let eye = camera_at.translation();
    let filter = SpatialQueryFilter::from_mask(GameLayer::World);
    for (tag, &NameTag(player), mut node, mut visibility, mut colour) in &mut tags {
        let Ok((at, seat, bodiless)) = players.get(player) else {
            commands.entity(tag).despawn();
            continue;
        };
        let head = at.translation() + Vec3::Y * (PLAYER_CAPSULE_LENGTH / 2.0 + PLAYER_RADIUS + NAME_TAG_LIFT);
        let distance = eye.distance(head);
        let strength = tag_strength(distance);
        let hidden = bodiless
            || strength <= 0.0
            || Dir3::new(head - eye).is_ok_and(|direction| spatial.cast_ray(eye, direction, distance, true, &filter).is_some());
        let on_screen = if hidden { None } else { camera.world_to_viewport(camera_at, head).ok() };
        let Some(screen) = on_screen else {
            show(&mut visibility, false);
            continue;
        };
        node.left = px(screen.x - NAME_TAG_WIDTH / 2.0);
        node.top = px(screen.y - NAME_TAG_FONT_PX);
        colour.0 = seat.colour().with_alpha(strength);
        show(&mut visibility, true);
    }
}

pub struct TeammatePlugin;

impl Plugin for TeammatePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<TeammateShot>().add_systems(
            Update,
            (fill_poses, spawn_bodies, animate, place_name_tags).chain().run_if(world_runs),
        );
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;

    use super::*;

    fn doing(pose: Pose, progress: f32) -> Doing {
        Doing { pose, speed: 0.0, phase: 0.0, kick: 0.0, clock: 0.0, progress }
    }

    #[test]
    fn a_pose_packs_into_two_bytes_and_back() {
        for action in [Action::None, Action::Reload, Action::Knife, Action::Throw, Action::Drink, Action::Revive] {
            for flags in 0..16u8 {
                let pose = Pose {
                    stance: Stance::Standing,
                    sprinting: flags & 1 != 0,
                    aiming: flags & 2 != 0,
                    airborne: flags & 4 != 0,
                    last_stand: flags & 8 != 0,
                    action,
                };
                assert_eq!(Pose::unpack(pose.pack()), pose);
            }
        }
        assert_eq!(Pose::default().pack(), 0);
        assert!(Pose { action: Action::Revive, ..default() }.pack() < 1 << 9, "fits in 9 bits");
    }

    #[test]
    fn a_pose_with_unknown_bits_reads_as_the_plain_case() {
        assert_eq!(Pose::unpack(0b111 << 6).action, Action::None);
    }

    #[test]
    fn name_tags_fade_with_distance() {
        assert_eq!(tag_strength(1.0), 1.0);
        assert_eq!(tag_strength(NAME_TAG_FADE_END + 5.0), 0.0);
        let middle = tag_strength((NAME_TAG_FADE_START + NAME_TAG_FADE_END) / 2.0);
        assert!((middle - 0.5).abs() < 1e-5);
    }

    #[test]
    fn aiming_brings_the_weapon_up_to_the_middle() {
        let hip = body(&doing(Pose::default(), 0.0));
        let aimed = body(&doing(Pose { aiming: true, ..default() }, 0.0));
        assert!(aimed.gun.x.abs() < hip.gun.x.abs());
        assert!(aimed.arms[1].pitch > hip.arms[1].pitch);
    }

    #[test]
    fn a_sprinter_carries_the_weapon_down_and_leans_in() {
        let hip = body(&doing(Pose::default(), 0.0));
        let sprint = body(&doing(Pose { sprinting: true, ..default() }, 0.0));
        assert!(sprint.gun.y < hip.gun.y && sprint.lean > hip.lean);
    }

    #[test]
    fn legs_step_against_each_other_when_moving_and_stand_still_when_not() {
        let still = body(&doing(Pose::default(), 0.0));
        assert_eq!(still.legs, [0.0, 0.0]);
        let walking = body(&Doing { speed: 3.0, phase: FRAC_PI_2, ..doing(Pose::default(), 0.0) });
        assert!(walking.legs[0] > 0.2 && walking.legs[1] < -0.2, "{:?}", walking.legs);
    }

    #[test]
    fn a_reload_drops_the_left_hand_for_a_magazine_and_brings_it_back() {
        let reload = Pose { action: Action::Reload, ..default() };
        let start = body(&doing(reload, 0.0)).arms[0].pitch;
        let fetching = body(&doing(reload, 0.45)).arms[0].pitch;
        let done = body(&doing(reload, 1.0)).arms[0].pitch;
        assert!(fetching < start - 0.5 && (done - start).abs() < 0.05);
    }

    #[test]
    fn a_knife_slash_raises_the_blade_then_brings_it_down() {
        let knife = Pose { action: Action::Knife, ..default() };
        let raised = body(&doing(knife, 0.3));
        let slashed = body(&doing(knife, 0.6));
        assert!(raised.arms[1].pitch > 2.0 && slashed.arms[1].pitch < 1.0);
        assert_eq!(raised.prop, Prop::Knife);
        assert!(!raised.gun_shown);
    }

    #[test]
    fn a_throw_lets_go_of_the_equipment_at_the_release() {
        let throw = Pose { action: Action::Throw, ..default() };
        assert_eq!(body(&doing(throw, THROW_RELEASE_AT - 0.05)).prop, Prop::Frag);
        assert_eq!(body(&doing(throw, THROW_RELEASE_AT + 0.05)).prop, Prop::Empty);
    }

    #[test]
    fn a_drink_raises_a_bottle_to_the_mouth() {
        let drink = Pose { action: Action::Drink, ..default() };
        let drinking = body(&doing(drink, 0.5));
        assert_eq!(drinking.prop, Prop::Bottle);
        assert!(drinking.arms[1].pitch > 2.0);
    }

    #[test]
    fn a_last_stand_crawls_face_down_and_a_revive_kneels() {
        let down = body(&doing(Pose { last_stand: true, ..default() }, 0.0));
        assert!(down.tilt > 1.0 && down.hips_y < 0.3);
        let kneel = body(&doing(Pose { action: Action::Revive, ..default() }, 0.0));
        assert!(kneel.hips_y < LEG.y - 0.2 && kneel.legs[0] > 1.0);
    }

    #[test]
    fn a_shot_kicks_the_weapon_back() {
        let still = body(&doing(Pose::default(), 0.0));
        let fired = body(&Doing { kick: 1.0, ..doing(Pose::default(), 0.0) });
        assert!(fired.gun.z > still.gun.z);
    }

    /// A Player in a bare world, with what `fill_poses` reads.
    fn filled(setup: impl FnOnce(&mut EntityWorldMut)) -> (World, Entity) {
        let mut world = World::new();
        let mut player = world.spawn((
            Player,
            Locomotion::default(),
            Loadout::new(STARTING_WEAPON),
            Knife::default(),
            Pose::default(),
            Shots::default(),
            HeldWeapon::default(),
        ));
        setup(&mut player);
        let player = player.id();
        world.run_system_once(fill_poses).unwrap();
        (world, player)
    }

    #[test]
    fn a_players_pose_is_filled_from_their_own_state() {
        let (world, player) = filled(|_| {});
        let pose = world.get::<Pose>(player).unwrap();
        assert!(pose.airborne, "a Player who hasn't landed yet is in the air");
        assert_eq!(pose.action, Action::None);
        assert!(!pose.sprinting && !pose.aiming && !pose.last_stand);

        let (world, player) = filled(|player| {
            player.get_mut::<Locomotion>().unwrap().sprinting = true;
            player.insert(Drinking::new(Perk::Juggernog));
        });
        let pose = world.get::<Pose>(player).unwrap();
        assert!(pose.sprinting);
        assert_eq!(pose.action, Action::Drink);
    }

    #[test]
    fn each_shot_steps_the_counter_once() {
        let (mut world, player) = filled(|_| {});
        world.run_system_once(fill_poses).unwrap();
        assert_eq!(world.get::<Shots>(player).unwrap().count(), 0, "no shot, no step");
        assert!(world.get_mut::<Loadout>(player).unwrap().try_fire(true, true, &[]));
        world.run_system_once(fill_poses).unwrap();
        world.run_system_once(fill_poses).unwrap();
        assert_eq!(world.get::<Shots>(player).unwrap().count(), 1);
    }
}
