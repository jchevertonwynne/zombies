//! What the datagrams of a Run carry: a Player's input going up, and how things stand coming
//! down. A snapshot is the full state of everything that changes fast, with no deltas, so a lost
//! one costs nothing. Players and their Points are in it, and every Zombie and Hellhound, each in
//! 12 bytes at most.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::net::protocol::Seat;
use crate::player::{Locomotion, MoveInput};
use crate::zombie::rig::{Activity, Swing};
use crate::tuning::WIRE_POSITION_STEP;

/// A count of the simulation's steps. Each machine counts its own.
pub type Tick = u32;

/// What a Player asked for in one tick: `MoveInput` as it goes up the wire.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub struct Step {
    /// Move axes, -1, 0 or 1: right, then forward.
    pub axes: [i8; 2],
    /// Jump, Sprint and Aim, in bits 0, 1 and 2.
    pub buttons: u8,
    pub yaw: f32,
    pub pitch: f32,
}

const JUMP: u8 = 1;
const SPRINT: u8 = 2;
const AIM: u8 = 4;

impl Step {
    pub fn of(input: &MoveInput) -> Self {
        Step {
            axes: [input.axes.x.round() as i8, input.axes.y.round() as i8],
            buttons: u8::from(input.jump) * JUMP
                | u8::from(input.sprint) * SPRINT
                | u8::from(input.aim) * AIM,
            yaw: input.yaw,
            pitch: input.pitch,
        }
    }

    /// The input this asks for. A Player is trusted, but an axis still only goes so far and a look
    /// is a real angle.
    pub fn input(self) -> MoveInput {
        let finite = |angle: f32| if angle.is_finite() { angle } else { 0.0 };
        MoveInput {
            axes: Vec2::new(
                f32::from(self.axes[0].clamp(-1, 1)),
                f32::from(self.axes[1].clamp(-1, 1)),
            ),
            jump: self.buttons & JUMP != 0,
            sprint: self.buttons & SPRINT != 0,
            aim: self.buttons & AIM != 0,
            yaw: finite(self.yaw),
            pitch: finite(self.pitch).clamp(-FRAC_PI_2, FRAC_PI_2),
        }
    }

    pub fn jump(self) -> bool {
        self.buttons & JUMP != 0
    }

    /// The same, with Jump pressed or not as given.
    pub fn with_jump(mut self, jump: bool) -> Self {
        self.buttons = self.buttons & !JUMP | u8::from(jump) * JUMP;
        self
    }
}

/// A Player's datagram to the Host, every tick.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Inputs {
    /// The tick of the last of `steps`.
    pub last: Tick,
    /// The last few ticks' input, oldest first.
    pub steps: Vec<Step>,
    /// What the Player's body is doing (`Pose::pack`) and their shot counter, which only they know.
    pub pose: u16,
    pub shots: u8,
}

/// The Host's datagram to a Player, with the receiver's own Player in full and the others packed.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Snapshot {
    /// The Host's tick when it was made.
    pub tick: Tick,
    /// The last of the receiver's ticks of input the Host had stepped their Player by.
    pub used: Tick,
    pub own: Own,
    /// Every other Player.
    pub players: Vec<Sight>,
    /// Every Zombie, and then every Hellhound.
    pub zombies: Vec<ZombieSight>,
    pub hellhounds: Vec<ZombieSight>,
    /// What each Special Zombie among them is, see `SpecialSight`.
    pub specials: Vec<SpecialSight>,
}

/// The receiver's own Player in full: what their movement step reads, so they can be put right and
/// stepped again.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Own {
    pub position: Vec3,
    pub velocity: Vec3,
    pub stamina: f32,
    /// Grounded, sprinting and winded, in bits 0, 1 and 2.
    pub bits: u8,
    pub points: u32,
}

const GROUNDED: u8 = 1;
const SPRINTING: u8 = 2;
const WINDED: u8 = 4;

impl Own {
    pub fn of(position: Vec3, locomotion: &Locomotion, points: u64) -> Self {
        Own {
            position,
            velocity: locomotion.velocity,
            stamina: locomotion.stamina,
            bits: u8::from(locomotion.grounded) * GROUNDED
                | u8::from(locomotion.sprinting) * SPRINTING
                | u8::from(locomotion.winded) * WINDED,
            points: points.min(u64::from(u32::MAX)) as u32,
        }
    }

    /// Puts the Player's movement back to this.
    pub fn put(&self, position: &mut Vec3, locomotion: &mut Locomotion) {
        *position = self.position;
        locomotion.velocity = self.velocity;
        locomotion.stamina = self.stamina;
        locomotion.grounded = self.bits & GROUNDED != 0;
        locomotion.sprinting = self.bits & SPRINTING != 0;
        locomotion.winded = self.bits & WINDED != 0;
    }
}

/// Another Player as a snapshot packs them, in about 15 bytes.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Sight {
    pub seat: Seat,
    /// Three `i16`s, see `pack_position`.
    pub position: [u8; 6],
    /// Which way they face and look, one byte each, see `pack_angle`.
    pub yaw: u8,
    pub pitch: u8,
    /// `Pose::pack`.
    pub pose: u16,
    pub shots: u8,
    pub points: u32,
}

/// A Zombie or a Hellhound as a snapshot packs them, in at most 12 bytes: its id (a varint of up
/// to 3), its position, its yaw and what it is doing.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct ZombieSight {
    /// Which it is, for as long as it lasts (see `ZombieIds`).
    pub id: u16,
    /// Three `i16`s, see `pack_position`.
    pub position: [u8; 6],
    pub yaw: u8,
    /// What it is doing, see `pack_anim`.
    pub anim: u8,
}

/// Which Special Zombie a Zombie is, and what is new about it, as a snapshot says it in 4 bytes at
/// most. Only Special Zombies are told of, each every snapshot, so a client that missed one still
/// learns what it is from the next. A Hellhound is told of by being in `Snapshot::hellhounds`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct SpecialSight {
    /// Which Zombie, see `ZombieSight`.
    pub id: u16,
    /// `pack_special`.
    pub body: u8,
}

/// The Special Zombies that have a body of their own on a client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecialBody {
    Napalm,
    Shrieker,
    Sapper,
    NovaCrawler,
}

/// Which Special Zombie it is in bits 0 to 2, and in bit 3 whether the Sapper has lost its helmet
/// or the Nova Crawler's gas has burst.
pub fn pack_special(body: SpecialBody, marked: bool) -> u8 {
    let code = match body {
        SpecialBody::Napalm => 1,
        SpecialBody::Shrieker => 2,
        SpecialBody::Sapper => 3,
        SpecialBody::NovaCrawler => 4,
    };
    code | u8::from(marked) << 3
}

/// What `pack_special` packed. A byte that is no Special Zombie is none.
pub fn unpack_special(byte: u8) -> Option<(SpecialBody, bool)> {
    let body = match byte & 0x7 {
        1 => SpecialBody::Napalm,
        2 => SpecialBody::Shrieker,
        3 => SpecialBody::Sapper,
        4 => SpecialBody::NovaCrawler,
        _ => return None,
    };
    Some((body, byte & 0x8 != 0))
}

impl ZombieSight {
    pub fn position(&self) -> Vec3 {
        unpack_position(self.position)
    }
}

/// What a Zombie is doing, and whether it crawls, as a byte: which of the nine things it is in
/// bits 0 to 3, a Crawler in bit 4, and how far through a swing, a tear or a fall it is, in eighths,
/// in bits 5 to 7.
pub fn pack_anim(activity: Activity, crawler: bool) -> u8 {
    let (code, progress) = match activity {
        Activity::Moving => (0, 0.0),
        Activity::Swinging { progress, .. } => (1, progress),
        Activity::Tearing { cycle } => (2, cycle),
        Activity::Vaulting => (3, 0.0),
        Activity::Rising => (4, 0.0),
        Activity::Screaming => (5, 0.0),
        Activity::Stunned => (6, 0.0),
        Activity::KnockedDown => (7, 0.0),
        Activity::Dying { t } => (8, t),
    };
    let eighths = (progress.clamp(0.0, 1.0) * 7.0).round() as u8;
    code | u8::from(crawler) << 4 | eighths << 5
}

/// What `pack_anim` packed. A swing is of the left arm: which swings is for the rig to take in
/// turn. A byte that is no activity is a Zombie walking.
pub fn unpack_anim(byte: u8) -> (Activity, bool) {
    let progress = f32::from(byte >> 5) / 7.0;
    let activity = match byte & 0xf {
        1 => Activity::Swinging { progress, arm: Swing::Arm(0) },
        2 => Activity::Tearing { cycle: progress },
        3 => Activity::Vaulting,
        4 => Activity::Rising,
        5 => Activity::Screaming,
        6 => Activity::Stunned,
        7 => Activity::KnockedDown,
        8 => Activity::Dying { t: progress },
        _ => Activity::Moving,
    };
    (activity, byte & 0x10 != 0)
}

/// The way a position goes on the wire: three `i16`s, `WIRE_POSITION_STEP` apart, as six bytes.
pub fn pack_position(position: Vec3) -> [u8; 6] {
    let mut bytes = [0; 6];
    for (axis, out) in [position.x, position.y, position.z].into_iter().zip(bytes.chunks_mut(2)) {
        let units = (axis / WIRE_POSITION_STEP).round().clamp(f32::from(i16::MIN), f32::from(i16::MAX));
        out.copy_from_slice(&(units as i16).to_le_bytes());
    }
    bytes
}

pub fn unpack_position(bytes: [u8; 6]) -> Vec3 {
    let axis = |i: usize| f32::from(i16::from_le_bytes([bytes[i], bytes[i + 1]])) * WIRE_POSITION_STEP;
    Vec3::new(axis(0), axis(2), axis(4))
}

/// A yaw (any angle) as a byte: a full turn in 256 steps.
pub fn pack_yaw(yaw: f32) -> u8 {
    ((yaw.rem_euclid(TAU) / TAU * 256.0).round() as u32 & 0xff) as u8
}

/// A yaw back, in -PI..PI.
pub fn unpack_yaw(byte: u8) -> f32 {
    let yaw = f32::from(byte) / 256.0 * TAU;
    if yaw > PI { yaw - TAU } else { yaw }
}

/// A pitch (up positive, within a quarter turn either way) as a byte.
pub fn pack_pitch(pitch: f32) -> u8 {
    ((pitch.clamp(-FRAC_PI_2, FRAC_PI_2) / PI + 0.5) * 255.0).round() as u8
}

pub fn unpack_pitch(byte: u8) -> f32 {
    (f32::from(byte) / 255.0 - 0.5) * PI
}

impl Sight {
    pub fn position(&self) -> Vec3 {
        unpack_position(self.position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::protocol::{DATAGRAM, encode};
    use crate::teammate::{Action, Pose, Stance};
    use crate::tuning::{ROOM_SEATS, SNAPSHOT_MAX_BYTES, SNAPSHOT_MAX_HELLHOUNDS, SNAPSHOT_MAX_SPECIALS, SNAPSHOT_MAX_ZOMBIES};

    #[test]
    fn a_position_is_told_to_a_centimetre() {
        let at = Vec3::new(-12.344, 1.8, 23.456);
        let back = unpack_position(pack_position(at));
        assert!(back.distance(at) < 0.01, "{back} for {at}");
        // Far past the Map's edge it holds at the edge instead of wrapping.
        assert!(unpack_position(pack_position(Vec3::splat(1e6))).x > 300.0);
        assert!(unpack_position(pack_position(Vec3::splat(-1e6))).x < -300.0);
    }

    #[test]
    fn a_look_is_told_to_a_byte() {
        for yaw in [-3.1, -1.0, 0.0, 0.5, 3.1, 7.0] {
            let back = unpack_yaw(pack_yaw(yaw));
            let off = (back - yaw).rem_euclid(TAU);
            assert!(off.min(TAU - off) < TAU / 256.0, "{back} for {yaw}");
        }
        for pitch in [-FRAC_PI_2, -0.4, 0.0, 0.7, FRAC_PI_2] {
            assert!((unpack_pitch(pack_pitch(pitch)) - pitch).abs() < PI / 255.0, "{pitch}");
        }
    }

    #[test]
    fn a_step_is_the_input_it_came_from() {
        let input = MoveInput { axes: Vec2::new(-1.0, 1.0), jump: true, sprint: false, aim: true, yaw: 1.25, pitch: -0.5 };
        assert_eq!(Step::of(&input).input(), input);
        let wild = Step { axes: [100, -100], buttons: 0, yaw: f32::NAN, pitch: 9.0 };
        let input = wild.input();
        assert_eq!(input.axes, Vec2::new(1.0, -1.0));
        assert_eq!((input.yaw, input.pitch), (0.0, FRAC_PI_2));
        assert!(!Step::default().with_jump(true).with_jump(false).jump());
    }

    /// A full table at its largest: every number as big as it gets.
    fn worst_snapshot() -> Snapshot {
        let sight = Sight {
            seat: 3,
            position: [0xff; 6],
            yaw: 255,
            pitch: 255,
            pose: Pose { stance: Stance::Standing, sprinting: true, aiming: true, airborne: true, last_stand: true, action: Action::Revive }.pack(),
            shots: 255,
            points: u32::MAX,
        };
        Snapshot {
            tick: Tick::MAX,
            used: Tick::MAX,
            own: Own {
                position: Vec3::splat(-123.456),
                velocity: Vec3::splat(-9.87),
                stamina: 1.2345,
                bits: 7,
                points: u32::MAX,
            },
            players: vec![sight; ROOM_SEATS - 1],
            zombies: vec![],
            hellhounds: vec![],
            specials: vec![],
        }
    }

    /// A Zombie at its largest: every number as big as it gets.
    fn worst_zombie() -> ZombieSight {
        ZombieSight { id: u16::MAX, position: [0xff; 6], yaw: 255, anim: 255 }
    }

    /// A full table at its largest, with a Horde at its largest: 56 Zombies and 2 Hellhounds, or
    /// as many Hellhounds as may be.
    fn worst_horde(hellhounds: usize) -> Snapshot {
        Snapshot {
            zombies: vec![worst_zombie(); SNAPSHOT_MAX_ZOMBIES],
            hellhounds: vec![worst_zombie(); hellhounds],
            specials: vec![SpecialSight { id: u16::MAX, body: 255 }; SNAPSHOT_MAX_SPECIALS],
            ..worst_snapshot()
        }
    }

    #[test]
    fn four_players_and_a_full_horde_fit_in_a_datagram() {
        for hellhounds in [2, SNAPSHOT_MAX_HELLHOUNDS] {
            let bytes = encode(&worst_horde(hellhounds)).len();
            assert!(bytes <= SNAPSHOT_MAX_BYTES, "{bytes} B with {hellhounds} Hellhounds");
        }
        assert!(SNAPSHOT_MAX_BYTES <= DATAGRAM);
        // The receiver's own Player is about 32 B, and each of the others about 15.
        let one = |players: usize| encode(&Snapshot { players: vec![worst_snapshot().players[0]; players], ..worst_snapshot() }).len();
        assert!(one(1) - one(0) <= 18, "a Player is {} B", one(1) - one(0));
        let usual = Snapshot { tick: 100_000, used: 99_990, own: Own { points: 5000, ..worst_snapshot().own }, players: vec![Sight { points: 5000, ..worst_snapshot().players[0] }], zombies: vec![], hellhounds: vec![], specials: vec![] };
        assert!(encode(&usual).len() <= 36 + 16 + 4 + 2, "{}", encode(&usual).len());
    }

    #[test]
    fn a_zombie_is_twelve_bytes_at_most() {
        let none = encode(&Snapshot { zombies: vec![], ..worst_snapshot() }).len();
        let one = encode(&Snapshot { zombies: vec![worst_zombie()], ..worst_snapshot() }).len();
        assert!(one - none <= 12, "a Zombie is {} B", one - none);
    }

    #[test]
    fn what_a_zombie_is_doing_is_told_to_a_byte() {
        let all = [
            Activity::Moving,
            Activity::Swinging { progress: 0.5, arm: Swing::Arm(1) },
            Activity::Tearing { cycle: 1.0 },
            Activity::Vaulting,
            Activity::Rising,
            Activity::Screaming,
            Activity::Stunned,
            Activity::KnockedDown,
            Activity::Dying { t: 0.25 },
        ];
        for activity in all {
            for crawler in [false, true] {
                let (back, crawls) = unpack_anim(pack_anim(activity, crawler));
                assert_eq!(crawls, crawler);
                match (activity, back) {
                    (Activity::Swinging { progress: a, .. }, Activity::Swinging { progress: b, .. })
                    | (Activity::Tearing { cycle: a }, Activity::Tearing { cycle: b })
                    | (Activity::Dying { t: a }, Activity::Dying { t: b }) => assert!((a - b).abs() < 0.08, "{a} for {b}"),
                    _ => assert_eq!(activity, back),
                }
            }
        }
        // A byte that is nothing known is a Zombie walking.
        assert_eq!(unpack_anim(0xf).0, Activity::Moving);
    }

    #[test]
    fn which_special_a_zombie_is_is_told_to_a_byte() {
        for body in [SpecialBody::Napalm, SpecialBody::Shrieker, SpecialBody::Sapper, SpecialBody::NovaCrawler] {
            for marked in [false, true] {
                assert_eq!(unpack_special(pack_special(body, marked)), Some((body, marked)));
            }
        }
        assert_eq!(unpack_special(0), None);
        let one = encode(&Snapshot { specials: vec![SpecialSight { id: u16::MAX, body: 255 }], ..worst_snapshot() }).len();
        assert!(one - encode(&worst_snapshot()).len() <= 4, "a special is {} B", one - encode(&worst_snapshot()).len());
    }

    #[test]
    fn inputs_with_their_resends_are_small() {
        let inputs = Inputs { last: Tick::MAX, steps: vec![Step { axes: [1, 1], buttons: 7, yaw: -3.0, pitch: 1.0 }; 4], pose: 0x1ff, shots: 255 };
        assert!(encode(&inputs).len() < 80, "{}", encode(&inputs).len());
    }
}
