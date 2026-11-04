//! What the datagrams of a Run carry: a Player's input going up, and how things stand coming
//! down. A snapshot is the full state of everything that changes fast, with no deltas, so a lost
//! one costs nothing. Players and their Points are in it; Zombies and Hellhounds are to join (see
//! `SNAPSHOT_ZOMBIE_RESERVE`).

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::net::protocol::Seat;
use crate::player::{Locomotion, MoveInput};
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
    use crate::tuning::{ROOM_SEATS, SNAPSHOT_ZOMBIE_RESERVE};

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
        }
    }

    #[test]
    fn a_full_table_of_players_leaves_room_for_the_zombies() {
        let bytes = encode(&worst_snapshot()).len();
        assert!(
            bytes + SNAPSHOT_ZOMBIE_RESERVE <= DATAGRAM,
            "Players take {bytes} B, leaving {} for Zombies",
            DATAGRAM - bytes
        );
        // The receiver's own Player is about 32 B, and each of the others about 15.
        let one = |players: usize| encode(&Snapshot { players: vec![worst_snapshot().players[0]; players], ..worst_snapshot() }).len();
        assert!(one(1) - one(0) <= 18, "a Player is {} B", one(1) - one(0));
        let usual = Snapshot { tick: 100_000, used: 99_990, own: Own { points: 5000, ..worst_snapshot().own }, players: vec![Sight { points: 5000, ..worst_snapshot().players[0] }] };
        assert!(encode(&usual).len() <= 36 + 16 + 4, "{}", encode(&usual).len());
    }

    #[test]
    fn inputs_with_their_resends_are_small() {
        let inputs = Inputs { last: Tick::MAX, steps: vec![Step { axes: [1, 1], buttons: 7, yaw: -3.0, pitch: 1.0 }; 4], pose: 0x1ff, shots: 255 };
        assert!(encode(&inputs).len() < 80, "{}", encode(&inputs).len());
    }
}
