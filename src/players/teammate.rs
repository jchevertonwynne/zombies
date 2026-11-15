//! Teammates on screen: every Player but the `LocalPlayer` is drawn as a slim blocky body, built
//! like the Zombie's (boxes on pivots, posed in code every frame), with a torso in their Seat's
//! colour, their held Weapon at its world scale, and a name tag over their head.
//!
//! What the body does comes from `Pose`, a few bits of state about the Player, and from `Shots`
//! and `HeldWeapon`. On the Host and in a two-Player world `fill_poses` fills them from the
//! Player's own state; on a client they will come off snapshots instead. The body reads only
//! those, so it draws the same either way. Purely visual: nothing here is in `HostSimulation`.

use std::f32::consts::{FRAC_PI_2, TAU};

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::weapons::ammo_mod::{AmmoMods, glint_colour};
use crate::weapons::arsenal::{WeaponClass, WeaponId};
use crate::weapons::attachment::Attachment;
use crate::weapons::equipment::Handling;
use crate::weapons::gun_model::{GunModels, Handheld, WORLD_SCALE, model_length};
use crate::players::last_stand::LastStand;
use crate::players::respawn::AwaitingRespawn;
use crate::layers::GameLayer;
use crate::weapons::muzzle::{aim_direction, muzzle_position};
use crate::economy::perk::{Drinking, Perk};
use crate::players::player::{LocalPlayer, Locomotion, MoveInput, Player, PlayerCamera, PlayerName, Seat, Stance, StanceFeel, transition_secs};
use crate::state::{InRun, world_runs};
use crate::tuning::*;
use crate::weapons::weapon::{Knife, Loadout, UPGRADED_PURPLE, WeaponCopy, glint_of};

/// The one-off thing a Player's hands are doing, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Action {
    #[default]
    None,
    Reload,
    Knife,
    Throw,
    Drink,
    /// Riding a Zipline: arms up on the trolley's bar, legs hanging.
    Ride,
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
    /// The two bytes a snapshot carries: stance in bits 0-1 (Standing, Crouched, Prone, the fourth
    /// value left free), then sprinting, aiming, airborne and Last Stand, then the action in bits
    /// 6-8. A Dive isn't in it: Prone ∧ airborne can only mean a Dive in flight.
    pub fn pack(self) -> u16 {
        let stance = match self.stance {
            Stance::Standing => 0,
            Stance::Crouched => 1,
            Stance::Prone => 2,
        };
        let action = match self.action {
            Action::None => 0,
            Action::Reload => 1,
            Action::Knife => 2,
            Action::Throw => 3,
            Action::Drink => 4,
            Action::Revive => 5,
            Action::Ride => 6,
        };
        stance
            | u16::from(self.sprinting) << 2
            | u16::from(self.aiming) << 3
            | u16::from(self.airborne) << 4
            | u16::from(self.last_stand) << 5
            | action << 6
    }

    /// The stance feedback this Pose shows, as the local Player's `Locomotion::feel` does: no
    /// Stance while Down, and a Dive while Prone and airborne.
    pub fn feel(&self) -> StanceFeel {
        StanceFeel {
            shown: (!self.last_stand).then_some(self.stance),
            diving: self.stance == Stance::Prone && self.airborne && !self.last_stand,
        }
    }

    /// How far the feet are below the body's centre in this Pose: half its height, which changes
    /// with the Stance at the start of a change, and is the prone body's while Down.
    pub fn feet_below_centre(&self) -> f32 {
        if self.last_stand { Stance::Prone.half_height() } else { self.stance.half_height() }
    }

    /// The Pose in `bits`. Anything it doesn't know is read as the plain case.
    pub fn unpack(bits: u16) -> Self {
        Self {
            stance: match bits & 0b11 {
                1 => Stance::Crouched,
                2 => Stance::Prone,
                _ => Stance::Standing,
            },
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
                6 => Action::Ride,
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
#[derive(Component, Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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
        let action = if locomotion.riding() {
            Action::Ride
        } else if drinking {
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
            stance: locomotion.stance,
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
/// Crouched: the hips' height, and the legs' turn from straight down, one thigh forward and one
/// back, as a kneel without the knee.
const CROUCH_HIPS_Y: f32 = 0.52;
const CROUCH_LEGS: [f32; 2] = [0.8, -0.8];
/// Prone: how far the body tips over, face down, and the hips' height, on the floor and in a
/// Dive's flight.
const PRONE_TILT: f32 = 1.45;
const PRONE_HIPS_Y: f32 = 0.18;
const DIVE_HIPS_Y: f32 = 0.35;
/// Prone, the Weapon comes up off the floor by this much, out ahead on the elbows.
const PRONE_GUN_LIFT: f32 = 0.15;
/// Down: tipped over onto the back (the opposite way to Prone), the legs turned up a little from
/// along the floor, and the head and the pistol pitched up from level, in the world, to look and
/// point down the body.
const DOWN_TILT: f32 = -1.4;
const DOWN_HIPS_Y: f32 = 0.22;
const DOWN_LEGS: f32 = 0.2;
const DOWN_HEAD_PITCH: f32 = 0.9;
const DOWN_PISTOL_PITCH: f32 = 0.35;
/// Lying on the back, the look turns the head and the pistol no further down than this, so the
/// pistol never points into the floor (the camera's own look down is free).
const DOWN_LOOK_DOWN_LIMIT: f32 = -0.4;
/// The Weapon up at the eye, both hands on it: as aiming holds it, and as a Last Stand holds the
/// pistol up over the chest.
const GUN_TWO_HANDED: Vec3 = Vec3::new(0.0, -0.02, -0.4);
const ARMS_TWO_HANDED: [Arm; 2] = [Arm { pitch: 1.35, yaw: 0.6 }, Arm { pitch: 1.5, yaw: 0.5 }];

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
    /// How deep into its Stance the body is, gliding: 0 Standing, 1 Crouched, 2 Prone.
    depth: f32,
}

/// A teammate body's depth into its Stance, 0 Standing, 1 Crouched, 2 Prone, gliding between them
/// over the same transition times the Player's own camera takes (Standing↔Prone through
/// Crouched), and snapping to Prone for a Dive in flight, which it reads off Prone ∧ airborne.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct StanceGlide {
    depth: f32,
    /// The Stance the glide set off from, and the one it heads for.
    from: Stance,
    to: Stance,
}

impl StanceGlide {
    /// Follows `pose` for `dt` seconds. Whether the body snapped (a Dive's launch), for it to take
    /// the new pose at once rather than ease into it.
    fn follow(&mut self, pose: &Pose, dt: f32) -> bool {
        if pose.feel().diving {
            let snapped = self.depth != f32::from(Stance::Prone.depth());
            *self = StanceGlide { depth: f32::from(Stance::Prone.depth()), from: Stance::Prone, to: Stance::Prone };
            return snapped;
        }
        if pose.stance != self.to {
            self.from = self.to;
            self.to = pose.stance;
        }
        let target = f32::from(self.to.depth());
        let span = (target - f32::from(self.from.depth())).abs().max(1.0);
        let step = span / transition_secs(self.from, self.to) * dt;
        let left = target - self.depth;
        self.depth = if left.abs() <= step { target } else { self.depth + step.copysign(left) };
        false
    }
}

/// How far the look turns a body's head and Weapon: as the Player looks, but lying on the back
/// no further down than `DOWN_LOOK_DOWN_LIMIT`.
fn look_pitch(pose: &Pose, pitch: f32) -> f32 {
    if pose.last_stand { pitch.max(DOWN_LOOK_DOWN_LIMIT) } else { pitch }
}

/// Whether a body tipped `from` heads for a tilt `to` on the other side of upright: face down to
/// the back, or back to face down. Its head and feet swap ends, so it goes over at once, rather
/// than rocking up through standing with its legs through the floor.
fn turns_over(from: f32, to: f32) -> bool {
    from * to < 0.0
}

/// Where a name tag floats, from where the head's pivot is: over the top of the standing body
/// while Standing, and following the head down from there.
fn name_tag_anchor(head: Vec3) -> Vec3 {
    head + Vec3::Y * (2.0 * Stance::Standing.half_height() - (LEG.y + TORSO.y) + NAME_TAG_LIFT)
}

/// Where every pivot of a body should head, before it's smoothed.
struct Body {
    /// The hips' height above the floor, and how far the whole body tips over from upright:
    /// positive face down (Prone), negative onto the back (Down).
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
    let Doing { pose, speed, phase, kick, progress, clock, depth } = *doing;
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
        body.gun = GUN_TWO_HANDED + Vec3::Z * 0.04 * kick;
        body.arms = ARMS_TWO_HANDED;
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
    // A jump; Prone and airborne is a Dive's flight instead, below.
    let diving = pose.feel().diving;
    if pose.airborne && !pose.last_stand && !diving {
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
        Action::Ride => {
            // Both hands up on the trolley's bar, the body hanging from it, legs straight.
            body.arms = [Arm { pitch: 3.0, yaw: 0.1 }, Arm { pitch: 3.0, yaw: -0.1 }];
            body.legs = [0.1, -0.05];
            body.gun_shown = false;
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
    let crouch = depth.clamp(0.0, 1.0);
    if crouch > 0.0 && pose.action != Action::Revive {
        // Knees bent: the hips down, one thigh forward and one back, the stride shortened. (A
        // Revive kneels already.)
        body.hips_y -= (LEG.y - CROUCH_HIPS_Y) * crouch;
        for side in 0..2 {
            body.legs[side] = body.legs[side] * (1.0 - 0.5 * crouch) + CROUCH_LEGS[side] * crouch;
        }
        body.lean += 0.15 * crouch;
    }
    let prone = (depth - 1.0).clamp(0.0, 1.0);
    if prone > 0.0 {
        // Face down along the floor, the head and the Weapon turned back up to look ahead. On the
        // floor they crawl, hauling on their elbows and pushing with their legs; in a Dive's
        // flight they're stretched out, arms reaching ahead.
        let flying = diving;
        let haul = if flying { 0.0 } else { stride * phase.sin() };
        body.tilt = body.tilt.lerp(PRONE_TILT, prone);
        body.hips_y = body.hips_y.lerp(if flying { DIVE_HIPS_Y } else { PRONE_HIPS_Y }, prone);
        body.lean = body.lean.lerp(0.04 * kick, prone);
        body.nod += 0.85 * PRONE_TILT * prone;
        body.aim += PRONE_TILT * prone;
        body.gun.y += PRONE_GUN_LIFT * prone;
        let legs = [0.35 * haul, -0.35 * haul];
        for side in 0..2 {
            body.legs[side] = body.legs[side].lerp(legs[side], prone);
        }
        if pose.action == Action::None && !pose.aiming {
            let arms = if flying {
                [Arm { pitch: 1.65, yaw: 0.3 }, Arm { pitch: 1.75, yaw: 0.25 }]
            } else {
                [Arm { pitch: 1.35 + 0.4 * haul, yaw: 0.6 }, Arm { pitch: 1.5 - 0.4 * haul, yaw: 0.5 }]
            };
            for side in 0..2 {
                body.arms[side] = Arm {
                    pitch: body.arms[side].pitch.lerp(arms[side].pitch, prone),
                    yaw: body.arms[side].yaw.lerp(arms[side].yaw, prone),
                };
            }
        }
    }
    if pose.last_stand {
        // On the back, feet first, the head lifted to look down the body and the pistol held up
        // over the chest in both hands; the Crawl pushes with the heels in turn. (Prone lies face
        // down; a Last Stand is one pose from any Stance, and a Down mid-Dive lands in it.)
        let push = 0.3 * stride * phase.sin();
        body.hips_y = DOWN_HIPS_Y;
        body.tilt = DOWN_TILT;
        body.lean = 0.0;
        body.nod = DOWN_HEAD_PITCH + DOWN_TILT;
        body.legs = [DOWN_LEGS + push, DOWN_LEGS - push];
        body.arms = ARMS_TWO_HANDED;
        body.gun = GUN_TWO_HANDED;
        body.gun_roll = 0.0;
        body.aim = DOWN_PISTOL_PITCH + DOWN_TILT;
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
    glide: StanceGlide,
    /// The tilt the body last headed for.
    tilt: f32,
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
            glide: StanceGlide::default(),
            tilt: 0.0,
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
        Action::None | Action::Revive | Action::Ride => None,
        Action::Reload => {
            let stats = held.id.stats();
            Some(stats.reload.secs(0, stats.magazine).max(0.5))
        }
        Action::Knife => Some(KNIFE_COOLDOWN_SECS),
        Action::Throw => Some(THROW_SECS),
        Action::Drink => Some(PERK_DRINK_SECS),
    }
}

/// The T-bar trolley a riding teammate hangs from: a stem up from their hands to the cable and a
/// bar across it, dark like the cable. (The Player's own is the hand on the view model.)
fn draw_trolleys(mut gizmos: Gizmos, riders: Query<(&Transform, &Pose), (With<Player>, Without<LocalPlayer>)>) {
    for (transform, pose) in &riders {
        if pose.action != Action::Ride {
            continue;
        }
        let cable = transform.translation + Vec3::Y * (ZIPLINE_HANG - pose.feet_below_centre());
        let side = transform.rotation * Vec3::X * 0.28;
        let bar = Color::srgb(0.05, 0.05, 0.06);
        gizmos.line(cable - Vec3::Y * 0.3, cable, bar);
        gizmos.line(cable - Vec3::Y * 0.3 - side, cable - Vec3::Y * 0.3 + side, bar);
        gizmos.line(cable - Vec3::Y * 0.05 - side * 0.4, cable - Vec3::Y * 0.05 + side * 0.4, bar);
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
        let crawling = pose.last_stand || pose.stance == Stance::Prone;
        let cycle = if crawling { 0.9 } else { 1.5 + 0.12 * rig.speed };
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

        // A Dive's launch snaps the body into its flight; the rest of the time it eases.
        let follow = if rig.glide.follow(pose, dt) { 1.0 } else { follow };

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
            depth: rig.glide.depth,
        });
        // Going over onto the back, or back onto the front, happens at once.
        let follow = if turns_over(rig.tilt, target.tilt) { 1.0 } else { follow };
        rig.tilt = target.tilt;

        let mut set = |part: Entity, f: &dyn Fn(&mut Transform, &mut Visibility)| {
            if let Ok((mut transform, mut visibility)) = parts.get_mut(part) {
                f(&mut transform, &mut visibility);
            }
        };
        // The feet stay on the floor as the body's centre moves with its Stance.
        let feet = -pose.feet_below_centre();
        set(rig.root, &|t, _| t.translation.y = feet);
        let tilt = Quat::from_rotation_x(-target.tilt);
        set(rig.hips, &|t, _| {
            t.translation.y = t.translation.y.lerp(target.hips_y, follow);
            t.rotation = t.rotation.slerp(tilt, follow);
        });
        let lean = Quat::from_rotation_x(-target.lean);
        set(rig.torso, &|t, _| t.rotation = t.rotation.slerp(lean, follow));
        // The head looks where the Player does.
        let look = look_pitch(pose, input.pitch);
        let nod = Quat::from_rotation_x(look * 0.7 + target.nod);
        set(rig.head, &|t, _| t.rotation = t.rotation.slerp(nod, follow));
        let aim = Quat::from_rotation_x(look * 0.8 + target.aim);
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
    players: Query<(&GlobalTransform, &Seat, Option<&Rig>, Has<AwaitingRespawn>), With<Player>>,
    heads: Query<&GlobalTransform, Without<Player>>,
    camera: Single<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    spatial: SpatialQuery,
) {
    let (camera, camera_at) = *camera;
    let eye = camera_at.translation();
    let filter = SpatialQueryFilter::from_mask(GameLayer::World);
    for (tag, &NameTag(player), mut node, mut visibility, mut colour) in &mut tags {
        let Ok((at, seat, rig, bodiless)) = players.get(player) else {
            commands.entity(tag).despawn();
            continue;
        };
        // Over the posed head; before the body is posed, over the standing one.
        let head = match rig.and_then(|rig| heads.get(rig.head).ok()) {
            Some(head) => name_tag_anchor(head.translation()),
            None => name_tag_anchor(at.translation() + Vec3::Y * (LEG.y + TORSO.y - Stance::Standing.half_height())),
        };
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
            (fill_poses, spawn_bodies, animate, place_name_tags, draw_trolleys).chain().run_if(world_runs),
        );
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;

    use super::*;

    fn doing(pose: Pose, progress: f32) -> Doing {
        Doing { pose, speed: 0.0, phase: 0.0, kick: 0.0, clock: 0.0, progress, depth: 0.0 }
    }

    #[test]
    fn a_pose_packs_into_two_bytes_and_back() {
        for (action, stance) in [Action::None, Action::Reload, Action::Knife, Action::Throw, Action::Drink, Action::Revive, Action::Ride]
            .into_iter()
            .flat_map(|action| [Stance::Standing, Stance::Crouched, Stance::Prone].map(|stance| (action, stance)))
        {
            for flags in 0..16u8 {
                let pose = Pose {
                    stance,
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
        // The fourth stance value is free, and reads as Standing until a Stance takes it.
        assert_eq!(Pose::unpack(0b11).stance, Stance::Standing);
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
    fn a_down_lies_on_the_back_and_a_revive_kneels() {
        let down = body(&doing(Pose { last_stand: true, ..default() }, 0.0));
        assert!(down.tilt < -1.0 && down.hips_y < 0.3, "tilt {} hips {}", down.tilt, down.hips_y);
        let kneel = body(&doing(Pose { action: Action::Revive, ..default() }, 0.0));
        assert!(kneel.hips_y < LEG.y - 0.2 && kneel.legs[0] > 1.0);
    }

    /// The pitch in the world, up positive, of a pivot on the torso (the head, the aim) from the
    /// body's tilt and lean and its own turn: each turns about the same axis, so they add (the
    /// hips and torso turn by their negatives).
    fn world_pitch(body: &Body, own: f32) -> f32 {
        own - body.tilt - body.lean
    }

    /// The same for a leg, which hangs from the hips, so only the tilt turns it.
    fn leg_pitch(body: &Body, leg: f32) -> f32 {
        leg - body.tilt
    }

    #[test]
    fn down_and_prone_lie_on_opposite_sides() {
        let down = body(&doing(Pose { last_stand: true, ..default() }, 0.0));
        let prone = body(&Doing { depth: 2.0, ..doing(at(Stance::Prone), 0.0) });
        assert!(down.tilt * prone.tilt < 0.0, "Down {} and Prone {} lie the same way up", down.tilt, prone.tilt);
    }

    #[test]
    fn a_down_teammate_holds_the_pistol_up_over_the_chest() {
        for action in [Action::None, Action::Reload] {
            let down = body(&doing(Pose { last_stand: true, action, ..default() }, 0.5));
            assert!(down.gun_shown, "{action:?}: the pistol hid");
            // Ahead and up from the chest, not into the floor or back over the head.
            let pitch = world_pitch(&down, down.aim);
            assert!(pitch > 0.1 && pitch < 0.8, "{action:?}: the pistol points at {pitch} rad");
            // The head lifts to look down the body, toward where they look.
            let head = world_pitch(&down, down.nod);
            assert!(head > 0.5 && head < FRAC_PI_2, "the head is at {head} rad");
        }
        for action in [Action::Knife, Action::Throw, Action::Drink] {
            assert!(!body(&doing(Pose { last_stand: true, action, ..default() }, 0.5)).gun_shown, "{action:?}");
        }
    }

    #[test]
    fn a_down_teammate_crawls_as_they_move_and_lies_still_when_not() {
        let down = doing(Pose { last_stand: true, ..default() }, 0.0);
        let still = body(&down);
        let speed = PLAYER_WALK_SPEED * LAST_STAND_CRAWL_MULT;
        let one = body(&Doing { speed, phase: FRAC_PI_2, ..down });
        let other = body(&Doing { speed, phase: -FRAC_PI_2, ..down });
        assert_eq!(still.legs[0], still.legs[1], "the heels push while still");
        assert!((one.legs[0] - other.legs[0]).abs() > 0.2, "the heels barely push");
        assert!(one.legs[0] > one.legs[1] && other.legs[0] < other.legs[1], "the heels don't take turns");
        // The legs reach ahead along the floor and never through it.
        for legs in [still.legs, one.legs, other.legs] {
            for leg in legs {
                let pitch = leg_pitch(&still, leg);
                let lowest = still.hips_y - LEG.y * pitch.cos() - LEG.x / 2.0;
                assert!(pitch.sin() > 0.9, "a leg doesn't reach ahead: {pitch}");
                assert!(lowest > -0.005, "a leg dips {lowest} m through the floor");
            }
        }
    }

    #[test]
    fn a_down_teammate_looking_down_never_points_the_pistol_into_the_floor() {
        let pose = Pose { last_stand: true, ..default() };
        let down = body(&doing(pose, 0.0));
        for look in [0.0, -0.3, -0.8, -FRAC_PI_2] {
            let look = look_pitch(&pose, look);
            let pistol = world_pitch(&down, down.aim + 0.8 * look);
            let head = world_pitch(&down, down.nod + 0.7 * look);
            assert!(pistol >= 0.0, "looking down, the pistol points {pistol} rad into the floor");
            assert!(head > 0.3, "looking down, the head folds into the chest at {head} rad");
        }
        // Looking up, and anyone not lying on their back, look as the Player does.
        assert_eq!(look_pitch(&pose, 0.6), 0.6);
        assert_eq!(look_pitch(&Pose::default(), -1.2), -1.2);
    }

    #[test]
    fn going_over_between_face_down_and_the_back_is_at_once() {
        let prone = body(&Doing { depth: 2.0, ..doing(at(Stance::Prone), 0.0) }).tilt;
        let down = body(&doing(Pose { last_stand: true, ..default() }, 0.0)).tilt;
        assert!(turns_over(prone, down), "Prone to a Last Stand rocked through upright");
        assert!(turns_over(down, prone));
        assert!(!turns_over(0.0, down), "a Down from Standing goes over at once");
        assert!(!turns_over(0.0, prone) && !turns_over(prone, prone) && !turns_over(down, down));
    }

    #[test]
    fn a_down_from_any_stance_or_mid_dive_lands_in_the_one_back_pose() {
        let plain = body(&doing(Pose { last_stand: true, ..default() }, 0.0));
        for (stance, airborne, depth) in [
            (Stance::Standing, false, 0.0),
            (Stance::Crouched, false, 1.0),
            (Stance::Prone, false, 2.0),
            (Stance::Prone, true, 2.0),
            (Stance::Standing, true, 0.0),
        ] {
            let down = body(&Doing { depth, ..doing(Pose { stance, airborne, last_stand: true, ..default() }, 0.0) });
            assert_eq!(
                (down.tilt, down.hips_y, down.lean, down.nod, down.aim, down.legs, down.gun, down.arms),
                (plain.tilt, plain.hips_y, plain.lean, plain.nod, plain.aim, plain.legs, plain.gun, plain.arms),
                "{stance:?}, airborne {airborne}"
            );
        }
    }

    #[test]
    fn a_shot_kicks_the_weapon_back() {
        let still = body(&doing(Pose::default(), 0.0));
        let fired = body(&Doing { kick: 1.0, ..doing(Pose::default(), 0.0) });
        assert!(fired.gun.z > still.gun.z);
    }

    fn at(stance: Stance) -> Pose {
        Pose { stance, airborne: false, ..default() }
    }

    /// Runs a glide from `from` until it settles on `to`, a frame of `dt` at a time: the seconds it
    /// took, and each depth on the way.
    fn glide(from: Stance, to: Stance, dt: f32) -> (f32, Vec<f32>) {
        let mut glide = StanceGlide::default();
        glide.follow(&at(from), 10.0);
        let mut depths = Vec::new();
        let mut secs = 0.0;
        while glide.depth != f32::from(to.depth()) && secs < 5.0 {
            glide.follow(&at(to), dt);
            depths.push(glide.depth);
            secs += dt;
        }
        (secs, depths)
    }

    #[test]
    fn the_body_glides_between_stances_over_the_players_own_transition_times() {
        let dt = 1.0 / 240.0;
        for (from, to, wanted) in [
            (Stance::Standing, Stance::Crouched, STANCE_CROUCH_SECS),
            (Stance::Crouched, Stance::Standing, STANCE_CROUCH_SECS),
            (Stance::Crouched, Stance::Prone, STANCE_PRONE_SECS),
            (Stance::Prone, Stance::Crouched, STANCE_PRONE_SECS),
            (Stance::Standing, Stance::Prone, STANCE_PRONE_FROM_STAND_SECS),
            (Stance::Prone, Stance::Standing, STANCE_PRONE_FROM_STAND_SECS),
        ] {
            let (secs, depths) = glide(from, to, dt);
            assert!((secs - wanted).abs() <= dt * 1.5, "{from:?} → {to:?} took {secs} s, not {wanted}");
            assert!(depths.windows(2).all(|pair| (pair[1] - pair[0]).abs() > 0.0), "{from:?} → {to:?} stalled");
        }
        // Standing to Prone passes through Crouched on the way, halfway through.
        let (_, depths) = glide(Stance::Standing, Stance::Prone, dt);
        let halfway = depths[depths.len() / 2 - 1];
        assert!((halfway - 1.0).abs() < 0.05, "halfway down was {halfway}");
    }

    #[test]
    fn a_dive_in_flight_snaps_prone_and_lands_into_the_crawl() {
        let mut glide = StanceGlide::default();
        glide.follow(&Pose { sprinting: true, ..at(Stance::Standing) }, 0.1);
        assert!(glide.follow(&Pose { airborne: true, ..at(Stance::Prone) }, 1.0 / 240.0), "the Dive didn't snap");
        assert_eq!(glide.depth, 2.0, "the Dive glided into its flight");
        let flight = body(&Doing { depth: glide.depth, ..doing(Pose { airborne: true, ..at(Stance::Prone) }, 0.0) });
        let crawl = body(&Doing { depth: 2.0, ..doing(at(Stance::Prone), 0.0) });
        assert!(flight.tilt > 1.2 && crawl.tilt > 1.2, "not laid out flat");
        assert!(flight.hips_y > crawl.hips_y, "the flight isn't off the floor");
        assert!(flight.arms[0].pitch > crawl.arms[0].pitch, "the arms don't reach out in flight");
        assert_eq!(flight.legs, [0.0, 0.0], "the legs aren't straight out behind");
        assert!(!glide.follow(&at(Stance::Prone), 1.0 / 240.0), "the landing snapped");
        assert_eq!(glide.depth, 2.0, "the landing moved");
    }

    #[test]
    fn a_jump_still_tucks_the_legs() {
        let jump = body(&doing(Pose { airborne: true, ..default() }, 0.0));
        assert_eq!(jump.legs, [0.7, -0.35]);
    }

    #[test]
    fn a_crouch_lowers_the_hips_and_bends_the_legs() {
        let stand = body(&doing(at(Stance::Standing), 0.0));
        let crouch = body(&Doing { depth: 1.0, ..doing(at(Stance::Crouched), 0.0) });
        assert!(crouch.hips_y < stand.hips_y - 0.2, "hips at {}", crouch.hips_y);
        assert!(crouch.legs[0] > 0.5 && crouch.legs[1] < -0.3, "{:?}", crouch.legs);
        assert!(crouch.lean > stand.lean);
        assert_eq!(crouch.tilt, 0.0);
        // Halfway down, halfway there.
        let half = body(&Doing { depth: 0.5, ..doing(at(Stance::Crouched), 0.0) });
        assert!(half.hips_y < stand.hips_y && half.hips_y > crouch.hips_y);
    }

    #[test]
    fn a_prone_body_lies_face_down_with_the_weapon_level() {
        let prone = body(&Doing { depth: 2.0, ..doing(at(Stance::Prone), 0.0) });
        assert!(prone.tilt > 1.2 && prone.hips_y < 0.3, "tilt {} hips {}", prone.tilt, prone.hips_y);
        // The aim pivot turns back by the tilt, so the Weapon points ahead, not into the floor.
        assert!((prone.aim - prone.tilt).abs() < 0.1, "aim {} against tilt {}", prone.aim, prone.tilt);
        assert!(prone.gun_shown);
    }

    #[test]
    fn a_prone_teammate_crawls_as_they_move_and_lies_still_when_not() {
        let prone = Doing { depth: 2.0, ..doing(at(Stance::Prone), 0.0) };
        let still = body(&prone);
        // At the prone Crawl's own speed, the haul shows from across the room.
        let speed = PLAYER_WALK_SPEED * PRONE_SPEED_MULT;
        let crawling = body(&Doing { speed, phase: FRAC_PI_2, ..prone });
        let other = body(&Doing { speed, phase: -FRAC_PI_2, ..prone });
        assert!(crawling.legs[0] != still.legs[0] && crawling.legs[0] * other.legs[0] < 0.0, "the legs don't crawl");
        assert!((crawling.legs[0] - other.legs[0]).abs() > 0.3, "the legs barely crawl");
        assert!((crawling.arms[0].pitch - other.arms[0].pitch).abs() > 0.4, "the arms barely haul");
    }

    #[test]
    fn idle_aim_kick_and_actions_pose_in_every_stance() {
        for depth in [0.0, 1.0, 2.0] {
            let stance = [Stance::Standing, Stance::Crouched, Stance::Prone][depth as usize];
            let with = |pose: Pose, progress| body(&Doing { depth, ..doing(Pose { stance, ..pose }, progress) });
            let reload = Pose { action: Action::Reload, airborne: false, ..default() };
            let start = with(reload, 0.0).arms[0].pitch;
            assert!(with(reload, 0.45).arms[0].pitch < start - 0.5, "{stance:?}: no magazine fetch");
            let knife = Pose { action: Action::Knife, airborne: false, ..default() };
            assert_eq!(with(knife, 0.3).prop, Prop::Knife, "{stance:?}");
            assert!(with(knife, 0.3).arms[1].pitch > 2.0, "{stance:?}: the blade didn't rise");
            let throw = Pose { action: Action::Throw, airborne: false, ..default() };
            assert_eq!(with(throw, THROW_RELEASE_AT + 0.05).prop, Prop::Empty, "{stance:?}");
            let drink = Pose { action: Action::Drink, airborne: false, ..default() };
            assert_eq!(with(drink, 0.5).prop, Prop::Bottle, "{stance:?}");
            assert!(with(Pose { aiming: true, airborne: false, ..default() }, 0.0).gun.x.abs() < 0.05, "{stance:?}");
            let kick = body(&Doing { depth, kick: 1.0, ..doing(Pose { stance, airborne: false, ..default() }, 0.0) });
            assert!(kick.gun.z > with(Pose { airborne: false, ..default() }, 0.0).gun.z, "{stance:?}: no kick");
            // A Revive's hands are on the teammate, not the Weapon.
            let revive = with(Pose { action: Action::Revive, airborne: false, ..default() }, 0.0);
            assert!(!revive.gun_shown, "{stance:?}: the hands aren't free to Revive");
        }
        // Down overrides any Stance with the Last Stand crawl.
        for depth in [0.0, 1.0, 2.0] {
            let down = body(&Doing { depth, ..doing(Pose { last_stand: true, airborne: false, ..default() }, 0.0) });
            let plain = body(&doing(Pose { last_stand: true, airborne: false, ..default() }, 0.0));
            assert_eq!((down.tilt, down.hips_y, down.nod), (plain.tilt, plain.hips_y, plain.nod));
        }
    }

    #[test]
    fn the_rig_hangs_from_the_feet_of_the_body_it_has() {
        assert_eq!(at(Stance::Standing).feet_below_centre(), Stance::Standing.half_height());
        assert_eq!(at(Stance::Crouched).feet_below_centre(), Stance::Crouched.half_height());
        assert_eq!(at(Stance::Prone).feet_below_centre(), Stance::Prone.half_height());
        // Down shares the prone body, whatever the stance field says.
        assert_eq!(Pose { last_stand: true, ..at(Stance::Standing) }.feet_below_centre(), Stance::Prone.half_height());
    }

    #[test]
    fn the_name_tag_rides_the_posed_head() {
        // Standing, it sits where it always has: over the top of the standing body.
        let standing_head = Vec3::Y * (LEG.y + TORSO.y);
        let tag = name_tag_anchor(standing_head);
        assert!((tag.y - (2.0 * Stance::Standing.half_height() + NAME_TAG_LIFT)).abs() < 1e-5);
        // Lower down, it follows the head down.
        assert!(name_tag_anchor(Vec3::Y * 0.3).y < tag.y - 1.0);
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
