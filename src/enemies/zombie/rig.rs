//! The Zombie's blocky body: boxes on a few pivots (hips, shoulders, legs and head), posed every
//! frame from what the Zombie is doing. It shambles with its arms out, swings at the Player, claws
//! at a Barrier's Planks and yanks them off, climbs through the window, claws its way out of the
//! ground, and throws its arms up as it falls dead. A Crawler drags itself along on its arms.
//!
//! Purely visual: the hit areas stay where they are, except the head, which leans with the body.
//! Only the rig's own entities are posed here, never the Zombie's Transform (see
//! `FrameTurnSystems`).

use std::f32::consts::{PI, TAU};

use bevy::prelude::*;

use super::{Dying, KnockedDown, Rising, Stunned, Zombie, ZombieAssets, body_half_height};
use crate::maps::barrier::{AtWindow, Entering};
use crate::enemies::special::Shrieker;
use crate::tuning::{CRAWLER_BODY_LENGTH, CRAWLER_RADIUS, ZOMBIE_FALL_SECS, ZOMBIE_HEAD_RADIUS, ZOMBIE_NECK_SIZE};

pub(super) const TORSO: Vec3 = Vec3::new(0.56, 0.66, 0.3);
pub(super) const ARM: Vec3 = Vec3::new(0.2, 0.5, 0.2);
pub(super) const HAND: Vec3 = Vec3::new(0.2, 0.16, 0.2);
pub(super) const LEG: Vec3 = Vec3::new(0.26, 0.7, 0.26);
/// The head: a cube as wide as the head's hit area, which is the same cube.
pub(super) const HEAD: f32 = 2.0 * ZOMBIE_HEAD_RADIUS;
/// A Crawler's body, lying along its facing.
pub(super) const CRAWLER_BODY: Vec3 = Vec3::new(0.5, 0.36, CRAWLER_BODY_LENGTH + 0.3);
/// How far the shoulders sit out from the middle, and above the hips.
const SHOULDER_X: f32 = (TORSO.x + ARM.x) / 2.0;
const SHOULDER_Y: f32 = TORSO.y - 0.08;
const LEG_X: f32 = 0.14;
/// How quickly limbs catch up with the pose they're heading for. Higher is snappier.
const FOLLOW_RATE: f32 = 18.0;
/// The fastest a Zombie is taken to move, so a teleport doesn't spin its legs.
const MAX_GAIT_SPEED: f32 = 8.0;

/// The pivots of one Zombie's rig, posed by `animate`.
#[derive(Component, Debug, Clone, Copy)]
#[require(Gait)]
pub(in crate::enemies) enum Rig {
    Standing {
        hips: Entity,
        /// Its head hit area, which carries the head's boxes.
        head: Entity,
        /// Its neck hit area, which leans with the upper body to stay under the head.
        neck: Entity,
        /// Left, then right.
        arms: [Entity; 2],
        legs: [Entity; 2],
    },
    Crawling {
        arms: [Entity; 2],
    },
    /// No boxes to pose: a Nova Crawler's, whose body is a single static mesh. It still has a `Gait`,
    /// so a skinned body has the Activity, speed and phase to pose from, on the Host and on a client.
    Bare,
}

/// How far through its walk cycle a Zombie is, and how fast it's going.
#[derive(Component, Debug, Default)]
pub(super) struct Gait {
    phase: f32,
    speed: f32,
    last: Option<Vec3>,
    clock: f32,
    /// Whether it was swinging last frame, and which arm swings (0 left, 1 right). Swings take turns.
    swinging: bool,
    arm: usize,
    /// What it was doing as of the last frame, for a skinned body to pose from.
    activity: Option<Activity>,
}

impl Gait {
    pub(super) fn activity(&self) -> Activity {
        self.activity.unwrap_or(Activity::Moving)
    }

    /// Radians through the walk cycle.
    pub(super) fn phase(&self) -> f32 {
        self.phase
    }

    /// How fast it is going, in metres a second.
    pub(super) fn speed(&self) -> f32 {
        self.speed
    }

    pub(super) fn clock(&self) -> f32 {
        self.clock
    }
}

/// A rig box dressed in the Zombie's outfit: its root's material, which the Specials and Turned
/// set to their own.
#[derive(Component)]
pub(super) struct Outfit(pub Entity);

/// What a Zombie is doing, as far as its body shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Activity {
    /// Walking, running or standing about.
    Moving,
    /// A swing at the Player, `progress` of the way (0 to 1) to landing, with `arm` (0 left,
    /// 1 right) or both.
    Swinging { progress: f32, arm: Swing },
    /// At a Barrier, `cycle` of the way (0 to 1) to its next tear or swipe.
    Tearing { cycle: f32 },
    /// Climbing in through a window, in pose `variant` (0 to 2). The Host picks it and tells it in
    /// the activity byte, so every machine shows the same one.
    Vaulting { variant: u8 },
    /// Clawing its way out of the ground.
    Rising,
    /// A Shrieker screaming.
    Screaming,
    Stunned,
    KnockedDown,
    /// Falling dead, `t` of the way (0 to 1) down. `headless` when a shot to the head killed it, so a
    /// skinned body shows no head.
    Dying { t: f32, headless: bool },
    /// Tipped off a Swing Bridge into the canal, `t` of the way (0 to 1) to the splash.
    Tipped { t: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Swing {
    Arm(usize),
    /// Both arms together, overhead, as the Sapper slams.
    Both,
}

/// One limb's turn from hanging straight down: `pitch` swings it forward (up past horizontal
/// at more than a quarter turn), `splay` out to the side, and `yaw` across the body. Written for
/// the right side; the left mirrors it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Limb {
    pub pitch: f32,
    pub splay: f32,
    pub yaw: f32,
}

impl Limb {
    pub(super) const fn pitch(pitch: f32) -> Self {
        Self { pitch, splay: 0.0, yaw: 0.0 }
    }

    const fn new(pitch: f32, splay: f32, yaw: f32) -> Self {
        Self { pitch, splay, yaw }
    }

    fn lerp(self, to: Limb, t: f32) -> Limb {
        Limb {
            pitch: self.pitch.lerp(to.pitch, t),
            splay: self.splay.lerp(to.splay, t),
            yaw: self.yaw.lerp(to.yaw, t),
        }
    }

    /// Its rotation on `side` (0 left, 1 right).
    pub(super) fn rotation(self, side: usize) -> Quat {
        let mirror = if side == 0 { -1.0 } else { 1.0 };
        Quat::from_euler(EulerRot::YZX, self.yaw * mirror, self.splay * mirror, self.pitch)
    }
}

/// A whole body's pose. `lean` tips the upper body forward from the hips, `twist` turns it
/// (positive brings the right shoulder forward), `nod` drops the head forward and `bob` lifts
/// the hips.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pose {
    pub lean: f32,
    pub twist: f32,
    pub nod: f32,
    pub bob: f32,
    /// Left, then right.
    pub arms: [Limb; 2],
    /// Left, then right, by pitch.
    pub legs: [f32; 2],
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Arms held out in front, the classic shamble.
const REACH: f32 = 1.35;

/// The standing pose for `activity`, `phase` (radians) through the walk cycle, at `stride` (0
/// standing to 1 full stride) and `run` (0 shambling to 1 sprinting), `clock` seconds in.
pub fn pose(activity: Activity, phase: f32, stride: f32, run: f32, clock: f32) -> Pose {
    let step = phase.sin();
    let moving = || {
        let amp = stride * (0.55 + 0.3 * run);
        let reach = |side: usize| {
            let alternate = if side == 0 { 1.0 } else { -1.0 };
            Limb::new(REACH + 0.12 * stride * step * alternate + 0.04 * (clock * 1.7).sin(), 0.08, 0.0)
        };
        // Sprinters pump their arms against their legs.
        let pump = |side: usize| {
            let alternate = if side == 0 { -1.0 } else { 1.0 };
            Limb::new(0.9 * amp * step * alternate + 0.2, 0.12, 0.0)
        };
        Pose {
            lean: 0.06 + 0.3 * run,
            twist: 0.12 * amp * step,
            nod: 0.08 * (clock * 1.3).sin(),
            bob: 0.04 * stride * step.abs(),
            arms: [reach(0).lerp(pump(0), run), reach(1).lerp(pump(1), run)],
            legs: [amp * step, -amp * step],
        }
    };
    match activity {
        Activity::Moving => moving(),
        Activity::Swinging { progress, arm } => {
            // A long wind-up overhead, then a fast strike down and across.
            let wind = smoothstep(progress / 0.75);
            let strike = smoothstep((progress - 0.75) / 0.25);
            let raised = Limb::new(2.7, 0.35, -0.2);
            let struck = Limb::new(0.8, 0.1, 0.6);
            let swinging = Limb::new(REACH, 0.08, 0.0).lerp(raised, wind).lerp(struck, strike);
            let lean = 0.05 - 0.15 * wind + 0.45 * strike;
            match arm {
                Swing::Both => Pose {
                    lean,
                    nod: -0.2 * wind + 0.3 * strike,
                    arms: [Limb { yaw: 0.0, ..swinging }; 2],
                    legs: [0.25, -0.15],
                    ..default()
                },
                Swing::Arm(side) => {
                    let toward = if side == 0 { -1.0 } else { 1.0 };
                    let mut arms = [Limb::new(REACH, 0.08, 0.0); 2];
                    arms[side] = swinging;
                    Pose {
                        lean,
                        twist: toward * (-0.35 * wind + 0.7 * strike),
                        nod: 0.15 * strike,
                        arms,
                        legs: if side == 0 { [0.3, -0.15] } else { [-0.15, 0.3] },
                        ..default()
                    }
                }
            }
        }
        Activity::Tearing { cycle } => {
            // Just yanked a Plank off: arms back, leaning away. Then reaching in and clawing at
            // the next one, and finally getting a grip and starting to pull.
            let yanked = Pose {
                lean: -0.2,
                nod: -0.15,
                arms: [Limb::new(0.55, 0.2, 0.0); 2],
                legs: [0.35, -0.2],
                ..default()
            };
            if cycle < 0.2 {
                return yanked;
            }
            let claw = |side: usize| {
                let offset = if side == 0 { 0.0 } else { PI };
                Limb::new(1.7 + 0.18 * (clock * 14.0 + offset).sin(), 0.05, 0.1)
            };
            let reaching = Pose {
                lean: 0.25,
                nod: 0.1,
                arms: [claw(0), claw(1)],
                legs: [0.2, -0.1],
                ..default()
            };
            let pull = smoothstep((cycle - 0.8) / 0.2);
            Pose {
                lean: reaching.lean.lerp(0.0, pull),
                arms: [
                    reaching.arms[0].lerp(Limb::new(1.2, 0.1, 0.0), pull),
                    reaching.arms[1].lerp(Limb::new(1.2, 0.1, 0.0), pull),
                ],
                ..reaching
            }
        }
        Activity::Vaulting { variant } => match variant {
            // Both hands planted on the Sill.
            0 => Pose {
                lean: 0.45,
                nod: 0.2,
                arms: [Limb::pitch(1.9); 2],
                legs: [1.3, 0.8],
                ..default()
            },
            // One hand and a knee, the other leg trailing.
            1 => Pose {
                lean: 0.3,
                nod: 0.1,
                arms: [Limb::pitch(2.1), Limb::pitch(1.2)],
                legs: [0.7, 1.4],
                ..default()
            },
            // A lunge over, leaning right out.
            _ => Pose {
                lean: 0.7,
                nod: 0.3,
                arms: [Limb::pitch(2.5), Limb::pitch(2.3)],
                legs: [1.0, 1.5],
                ..default()
            },
        },
        Activity::Rising => {
            let claw = |side: usize| {
                let offset = if side == 0 { 0.0 } else { PI };
                Limb::new(2.6 + 0.35 * (clock * 9.0 + offset).sin(), 0.25, 0.0)
            };
            Pose {
                lean: -0.1,
                nod: -0.3,
                arms: [claw(0), claw(1)],
                ..default()
            }
        }
        Activity::Screaming => Pose {
            lean: -0.25,
            nod: -0.5,
            arms: [Limb::new(1.2, 1.1, -0.3); 2],
            legs: [0.15, -0.15],
            ..default()
        },
        Activity::Stunned => {
            let shake = 0.08 * (clock * 25.0).sin();
            Pose {
                lean: 0.15,
                nod: 0.4,
                arms: [Limb::new(0.25 + shake, 0.15, 0.0), Limb::new(0.25 - shake, 0.15, 0.0)],
                ..default()
            }
        }
        Activity::KnockedDown => Pose {
            nod: -0.3,
            arms: [Limb::new(2.4, 0.7, 0.0); 2],
            legs: [0.5, -0.2],
            ..default()
        },
        Activity::Tipped { t } => {
            // Arms flailing over its head as it goes over the side.
            let flail = (t * 24.0).sin() * 0.35;
            Pose {
                nod: -0.3,
                arms: [Limb::new(2.7 + flail, 0.5, 0.0), Limb::new(2.7 - flail, 0.5, 0.0)],
                legs: [0.6 * flail, -0.6 * flail],
                ..default()
            }
        }
        Activity::Dying { t, .. } => {
            // Arms flung up over its head as it goes over backwards.
            let t = smoothstep(t);
            Pose {
                nod: -0.4 * t,
                arms: [Limb::new(REACH, 0.08, 0.0).lerp(Limb::new(2.9, 0.4, 0.0), t); 2],
                legs: [0.5 * t, 0.15 * t],
                ..default()
            }
        }
    }
}

/// A Crawler's arms: reaching out in turn and dragging it along, or one raised to swipe.
pub fn crawl(activity: Activity, phase: f32, stride: f32, clock: f32) -> [Limb; 2] {
    let drag = |side: usize| {
        let offset = if side == 0 { 0.0 } else { PI };
        let pull = (phase + offset).sin();
        // Flat along the floor, lifting and reaching while the other one pulls.
        Limb::new(1.45 + 0.35 * stride * pull + 0.03 * (clock * 2.0).sin(), 0.2, 0.15 * stride * pull)
    };
    match activity {
        Activity::Swinging { progress, arm } => {
            let side = match arm {
                Swing::Arm(side) => side,
                Swing::Both => 1,
            };
            let mut arms = [drag(0), drag(1)];
            let wind = smoothstep(progress / 0.75);
            let strike = smoothstep((progress - 0.75) / 0.25);
            arms[side] = Limb::new(1.45 + 0.9 * wind - 1.1 * strike, 0.3, 0.3 * strike);
            arms
        }
        Activity::Dying { t, .. } | Activity::Tipped { t } => [Limb::new(1.45, 0.2, 0.0).lerp(Limb::new(1.0, 0.8, 0.0), t); 2],
        _ => [drag(0), drag(1)],
    }
}

/// The parts of a standing Zombie besides its hit areas: hips carrying the torso and both
/// shoulders, and two legs. `head` is its head hit area, which wears `face`. `root` is the Zombie.
pub(super) fn spawn_standing(
    zombie: &mut ChildSpawnerCommands,
    assets: &ZombieAssets,
    root: Entity,
    head: Entity,
    neck: Entity,
) -> Rig {
    let hips_y = hips_height();
    let arm = |zombie: &mut ChildSpawnerCommands, side: usize| {
        let x = if side == 0 { -SHOULDER_X } else { SHOULDER_X };
        zombie
            .spawn((Transform::from_xyz(x, SHOULDER_Y, 0.0), Visibility::default()))
            .with_children(|shoulder| {
                shoulder.spawn((
                    Outfit(root),
                    Mesh3d(assets.arm.clone()),
                    MeshMaterial3d(assets.clothes.clone()),
                    Transform::from_xyz(0.0, 0.06 - ARM.y / 2.0, 0.0),
                ));
                shoulder.spawn((
                    Mesh3d(assets.hand.clone()),
                    MeshMaterial3d(assets.skin.clone()),
                    Transform::from_xyz(0.0, 0.06 - ARM.y - HAND.y / 2.0, 0.0),
                ));
            })
            .id()
    };
    let mut arms = [Entity::PLACEHOLDER; 2];
    let hips = zombie
        .spawn((Transform::from_xyz(0.0, hips_y, 0.0), Visibility::default()))
        .with_children(|hips| {
            hips.spawn((
                Outfit(root),
                Mesh3d(assets.torso.clone()),
                MeshMaterial3d(assets.clothes.clone()),
                Transform::from_xyz(0.0, TORSO.y / 2.0, 0.0),
            ));
            arms = [arm(hips, 0), arm(hips, 1)];
        })
        .id();
    let legs = [-LEG_X, LEG_X].map(|x| {
        zombie
            .spawn((Transform::from_xyz(x, hips_y, 0.0), Visibility::default()))
            .with_child((
                Mesh3d(assets.leg.clone()),
                MeshMaterial3d(assets.trousers.clone()),
                Transform::from_xyz(0.0, -LEG.y / 2.0, 0.0),
            ))
            .id()
    });
    Rig::Standing { hips, head, neck, arms, legs }
}

/// A Crawler's arms, reaching out from the front of its body, and the stumps of its legs.
pub(super) fn spawn_crawling(crawler: &mut ChildSpawnerCommands, assets: &ZombieAssets, root: Entity) -> Rig {
    let front = -(CRAWLER_BODY_LENGTH / 2.0 + CRAWLER_RADIUS);
    let arms = [-1.0, 1.0].map(|x| {
        crawler
            .spawn((Transform::from_xyz(x * (CRAWLER_BODY.x + ARM.x) / 2.0, 0.0, front + 0.3), Visibility::default()))
            .with_children(|shoulder| {
                shoulder.spawn((
                    Outfit(root),
                    Mesh3d(assets.arm.clone()),
                    MeshMaterial3d(assets.clothes.clone()),
                    Transform::from_xyz(0.0, 0.06 - ARM.y / 2.0, 0.0),
                ));
                shoulder.spawn((
                    Mesh3d(assets.hand.clone()),
                    MeshMaterial3d(assets.skin.clone()),
                    Transform::from_xyz(0.0, 0.06 - ARM.y - HAND.y / 2.0, 0.0),
                ));
            })
            .id()
    });
    for x in [-LEG_X, LEG_X] {
        crawler.spawn((
            Mesh3d(assets.stump.clone()),
            MeshMaterial3d(assets.trousers.clone()),
            Transform::from_xyz(x, -0.12, CRAWLER_BODY.z / 2.0 + 0.08),
        ));
    }
    Rig::Crawling { arms }
}

/// Eyes and a mouth on the front (−Z) of a head `size` across.
pub(super) fn face(head: &mut ChildSpawnerCommands, assets: &ZombieAssets, size: f32) {
    let front = -size / 2.0 - 0.005;
    for x in [-0.09, 0.09] {
        head.spawn((
            Mesh3d(assets.eye.clone()),
            MeshMaterial3d(assets.eyes.clone()),
            Transform::from_xyz(x, 0.04, front),
        ));
    }
    head.spawn((
        Mesh3d(assets.mouth.clone()),
        MeshMaterial3d(assets.mouth_mat.clone()),
        Transform::from_xyz(0.02, -0.1, front),
    ));
}

/// The hips' height above the body's centre: the top of the legs.
fn hips_height() -> f32 {
    LEG.y - body_half_height()
}

/// Where the neck's hit area sits on a standing Zombie at rest: just under the head.
fn neck_rest() -> Vec3 {
    head_rest() - Vec3::Y * (ZOMBIE_HEAD_RADIUS + ZOMBIE_NECK_SIZE.y / 2.0)
}

/// Where the head's hit area sits on a standing Zombie at rest.
pub(super) fn head_rest() -> Vec3 {
    Vec3::Y * (body_half_height() + ZOMBIE_HEAD_RADIUS * 0.5)
}

/// What a puppet Zombie is doing, as the Host told it: the rig shows this and nothing it could
/// work out from the Zombie's own state, which on a client is only a body.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Shown(pub Activity);

/// What a Zombie is doing, for its rig. Turns a new swing to the other arm. A puppet is doing
/// what it was `told`.
fn activity(
    gait: &mut Gait,
    told: Option<Activity>,
    zombie: Option<&Zombie>,
    entering: Option<&Entering>,
    rising: bool,
    stunned: bool,
    down: bool,
    dying: Option<&Dying>,
    screaming: bool,
) -> Activity {
    let swinging = match told {
        Some(told) => matches!(told, Activity::Swinging { .. }),
        None => zombie.is_some_and(|zombie| zombie.wind_up.is_some()),
    };
    if swinging && !gait.swinging {
        gait.arm = 1 - gait.arm;
    }
    gait.swinging = swinging;
    match told {
        // Which arm swings is the rig's to take in turn: the Host doesn't say.
        Some(Activity::Swinging { progress, arm: Swing::Arm(_) }) => {
            Activity::Swinging { progress, arm: Swing::Arm(gait.arm) }
        }
        Some(told) => told,
        None => describe(gait.arm, zombie, entering, rising, stunned, down, dying, screaming),
    }
}

/// What a Zombie is doing, from its own state, a swing being with `arm` (0 left, 1 right).
pub fn describe(
    arm: usize,
    zombie: Option<&Zombie>,
    entering: Option<&Entering>,
    rising: bool,
    stunned: bool,
    down: bool,
    dying: Option<&Dying>,
    screaming: bool,
) -> Activity {
    let wind_up = zombie.and_then(|zombie| zombie.wind_up.map(|left| (zombie, left)));
    if let Some(dying) = dying {
        if dying.killed_by() == Some(crate::enemies::zombie::Attack::Swing) {
            return Activity::Tipped { t: (dying.elapsed / crate::tuning::TIPPED_FALL_SECS).min(1.0) };
        }
        return Activity::Dying { t: (dying.elapsed / ZOMBIE_FALL_SECS).min(1.0), headless: dying.headless() };
    }
    if down {
        return Activity::KnockedDown;
    }
    if stunned {
        return Activity::Stunned;
    }
    if rising {
        return Activity::Rising;
    }
    if screaming {
        return Activity::Screaming;
    }
    if let Some(entering) = entering {
        match entering.at_window() {
            AtWindow::Approaching => {}
            AtWindow::Working { cycle } => return Activity::Tearing { cycle },
            AtWindow::Vaulting { variant } => return Activity::Vaulting { variant },
        }
    }
    if let Some((zombie, left)) = wind_up {
        let total = zombie.attack_stats().1;
        let arm = if zombie.is_sapper() { Swing::Both } else { Swing::Arm(arm) };
        return Activity::Swinging { progress: (1.0 - left / total).clamp(0.0, 1.0), arm };
    }
    Activity::Moving
}

/// Poses every rig from what its Zombie is doing and how fast it's moving.
pub(super) fn animate(
    mut zombies: Query<(
        Entity,
        &Rig,
        &mut Gait,
        &Transform,
        Option<&Zombie>,
        Option<&Entering>,
        Has<Rising>,
        Has<Stunned>,
        Has<KnockedDown>,
        Option<&Dying>,
        Option<&Shrieker>,
        Option<&Shown>,
    )>,
    mut parts: Query<&mut Transform, Without<Rig>>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let follow = 1.0 - (-FOLLOW_RATE * dt).exp();
    for (entity, rig, mut gait, transform, zombie, entering, rising, stunned, down, dying, shrieker, shown) in &mut zombies {
        let at = transform.translation;
        let moved = gait.last.map_or(0.0, |last| Vec2::new(at.x - last.x, at.z - last.z).length());
        gait.last = Some(at);
        let speed = (moved / dt).min(MAX_GAIT_SPEED);
        gait.speed = gait.speed.lerp(speed, 1.0 - (-10.0 * dt).exp());
        gait.clock += dt;
        let crawling = matches!(rig, Rig::Crawling { .. } | Rig::Bare);
        // A full cycle (two steps) covers more ground the faster it goes.
        let cycle = if crawling { 0.7 } else { 1.3 + 0.2 * gait.speed };
        gait.phase = (gait.phase + moved.min(MAX_GAIT_SPEED * dt) / cycle * TAU) % TAU;
        let stride = (gait.speed / 1.2).min(1.0);
        let run = ((gait.speed - 2.5) / 2.5).clamp(0.0, 1.0);
        let screaming = shrieker.is_some_and(Shrieker::screaming);
        let activity = activity(&mut gait, shown.map(|shown| shown.0), zombie, entering, rising, stunned, down, dying, screaming);
        gait.activity = Some(activity);

        let mut set = |part: Entity, f: &dyn Fn(&mut Transform)| {
            if let Ok(mut transform) = parts.get_mut(part) {
                f(&mut transform);
            }
        };
        match *rig {
            Rig::Standing { hips, head, neck, arms, legs } => {
                let pose = pose(activity, gait.phase, stride, run, gait.clock);
                let upper = Quat::from_rotation_y(pose.twist) * Quat::from_rotation_x(-pose.lean);
                let hips_at = Vec3::Y * (hips_height() + pose.bob);
                set(hips, &|t| {
                    t.translation = hips_at;
                    t.rotation = t.rotation.slerp(upper, follow);
                });
                let leaned = parts.get(hips).map_or(upper, |t| t.rotation);
                let mut set = |part: Entity, f: &dyn Fn(&mut Transform)| {
                    if let Ok(mut transform) = parts.get_mut(part) {
                        f(&mut transform);
                    }
                };
                set(neck, &|t| {
                    t.rotation = leaned;
                    t.translation = hips_at + leaned * (neck_rest() - Vec3::Y * hips_height());
                });
                // The head rides on the upper body, with its own nod and a tilt of its own.
                let tilt = ((entity.index_u32() % 7) as f32 - 3.0) * 0.06;
                let head_rotation = upper * Quat::from_rotation_x(-pose.nod) * Quat::from_rotation_z(tilt);
                set(head, &|t| {
                    t.rotation = t.rotation.slerp(head_rotation, follow);
                    t.translation = hips_at + t.rotation * (head_rest() - Vec3::Y * hips_height());
                });
                for side in 0..2 {
                    let arm = pose.arms[side].rotation(side);
                    set(arms[side], &|t| t.rotation = t.rotation.slerp(arm, follow));
                    let leg = Quat::from_rotation_x(pose.legs[side]);
                    set(legs[side], &|t| t.rotation = t.rotation.slerp(leg, follow));
                }
            }
            Rig::Bare => {}
            Rig::Crawling { arms } => {
                let limbs = crawl(activity, gait.phase, stride, gait.clock);
                for side in 0..2 {
                    let arm = limbs[side].rotation(side);
                    set(arms[side], &|t| t.rotation = t.rotation.slerp(arm, follow));
                }
            }
        }
    }
}

/// Dresses outfit boxes in their Zombie's current material.
pub(super) fn wear_outfits(
    mut parts: Query<(&Outfit, &mut MeshMaterial3d<StandardMaterial>)>,
    roots: Query<&MeshMaterial3d<StandardMaterial>, Without<Outfit>>,
) {
    for (outfit, mut material) in &mut parts {
        if let Ok(worn) = roots.get(outfit.0)
            && material.0 != worn.0
        {
            material.0 = worn.0.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shambling_zombie_holds_its_arms_out_and_steps_with_alternate_legs() {
        let pose = pose(Activity::Moving, PI / 2.0, 1.0, 0.0, 0.0);
        for arm in pose.arms {
            assert!((arm.pitch - REACH).abs() < 0.25, "arms out in front: {arm:?}");
        }
        assert!(pose.legs[0] > 0.3 && pose.legs[1] < -0.3, "one leg forward, one back: {:?}", pose.legs);
    }

    #[test]
    fn a_sprinter_pumps_its_arms_against_its_legs_and_leans_in() {
        let pose = pose(Activity::Moving, PI / 2.0, 1.0, 1.0, 0.0);
        assert!(pose.arms[0].pitch < 0.0 && pose.arms[1].pitch > 0.0, "{:?}", pose.arms);
        assert!(pose.legs[0] > 0.0, "left leg forward while the left arm goes back");
        assert!(pose.lean > 0.3);
    }

    #[test]
    fn a_standing_zombie_keeps_its_feet_together() {
        let pose = pose(Activity::Moving, 1.0, 0.0, 0.0, 3.0);
        assert_eq!(pose.legs, [0.0, 0.0]);
    }

    #[test]
    fn a_swing_raises_one_arm_overhead_then_brings_it_down() {
        let arm = Swing::Arm(1);
        let raised = pose(Activity::Swinging { progress: 0.75, arm }, 0.0, 0.0, 0.0, 0.0);
        assert!(raised.arms[1].pitch > 2.5, "overhead at the top of the wind-up");
        assert!((raised.arms[0].pitch - REACH).abs() < 1e-3, "the other arm stays out");
        let landed = pose(Activity::Swinging { progress: 1.0, arm }, 0.0, 0.0, 0.0, 0.0);
        assert!(landed.arms[1].pitch < 1.0, "down and across once it lands");
        assert!(landed.lean > raised.lean, "leaning into the blow");
    }

    #[test]
    fn the_sapper_slams_with_both_arms() {
        let raised = pose(Activity::Swinging { progress: 0.75, arm: Swing::Both }, 0.0, 0.0, 0.0, 0.0);
        assert!(raised.arms.iter().all(|arm| arm.pitch > 2.5));
    }

    #[test]
    fn tearing_reaches_in_then_yanks_the_plank_back() {
        let reaching = pose(Activity::Tearing { cycle: 0.5 }, 0.0, 0.0, 0.0, 0.0);
        let yanked = pose(Activity::Tearing { cycle: 0.05 }, 0.0, 0.0, 0.0, 0.0);
        assert!(reaching.lean > 0.0 && yanked.lean < 0.0);
        for side in 0..2 {
            assert!(reaching.arms[side].pitch > 1.4);
            assert!(yanked.arms[side].pitch < 0.8);
        }
    }

    #[test]
    fn a_dying_zombie_throws_its_arms_up() {
        let start = pose(Activity::Dying { t: 0.0, headless: false }, 0.0, 0.0, 0.0, 0.0);
        let end = pose(Activity::Dying { t: 1.0, headless: false }, 0.0, 0.0, 0.0, 0.0);
        assert!(end.arms[0].pitch > start.arms[0].pitch + 1.0);
    }

    #[test]
    fn a_crawler_reaches_out_with_one_arm_while_the_other_pulls() {
        let arms = crawl(Activity::Moving, PI / 2.0, 1.0, 0.0);
        assert!(arms[0].pitch > arms[1].pitch + 0.5, "{arms:?}");
    }

    /// A Nova Crawler has no boxes to pose, but still keeps its `Gait`, so a skinned body (on the Host
    /// and, from the Activity the Host told, on a client) has something to pose from.
    #[test]
    fn a_nova_crawlers_bare_rig_still_follows_what_it_is_told_it_does() {
        use std::time::Duration;
        let mut app = App::new();
        app.init_resource::<Time>().add_systems(Update, animate);
        let nova = app
            .world_mut()
            .spawn((Rig::Bare, Zombie::nova_crawler(10.0, Vec3::ZERO), Transform::default(), Shown(Activity::Stunned)))
            .id();
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(16));
        app.update();
        assert_eq!(app.world().get::<Gait>(nova).unwrap().activity(), Activity::Stunned);
    }

    #[test]
    fn a_left_limb_mirrors_the_right() {
        let limb = Limb::new(1.0, 0.4, 0.3);
        let right = limb.rotation(1) * Vec3::NEG_Y;
        let left = limb.rotation(0) * Vec3::NEG_Y;
        assert!((right.x + left.x).abs() < 1e-5 && (right.y - left.y).abs() < 1e-5 && (right.z - left.z).abs() < 1e-5);
        assert!(right.z < 0.0, "pitched forward");
        let splayed = Limb::new(1.0, 0.4, 0.0).rotation(1) * Vec3::NEG_Y;
        assert!(splayed.x > 0.0, "splayed outward on the right");
        let across = Limb::new(1.4, 0.0, 0.6).rotation(1) * Vec3::NEG_Y;
        assert!(across.x < 0.0, "yawed across the body");
    }
}
