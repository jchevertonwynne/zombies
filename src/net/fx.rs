//! Cosmetic effects over the network: the Host-only flashes, rings and arcs.
//!
//! Ammo Mod flashes, Proc and Set flashes, Cleave's Frost patches and Supply Drop's shell are
//! made only by Host systems. Each puts a record of itself in its resource's outbox as it is
//! drawn; the Host sends the frame's records as one `ToPlayer::Fx` batch, and a client pushes each
//! into the same resource, so the existing `draw_*` systems show it. None of it is in the
//! snapshot. Over `NET_FX_MAX_PER_FRAME` in a frame, the newest are dropped.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::hits::Hits;
use super::play::{Prediction, Roster, Wire};
use super::protocol::{ToPlayer, encode};
use super::state::{pack_position, unpack_position, wire_origin};
use crate::authority::HostSimulation;
use crate::maps::map::MapId;
use crate::run::gauntlet::duos::ShellView;
use crate::run::gauntlet::procs::ProcFlashes;
use crate::run::gauntlet::sets::{FrostPatch, SetFlashes};
use crate::state::world_runs;
use crate::tuning::NET_FX_MAX_PER_FRAME;
use crate::weapons::ammo_mod::Flashes;

/// One cosmetic effect as the machine that draws it has it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fx {
    /// An Ammo Mod's line (a Shock chain link, a Fireworks round).
    AmmoLine { from: Vec3, to: Vec3, colour: Color },
    /// An Ammo Mod's burst growing out to `radius`.
    AmmoBurst { at: Vec3, radius: f32, colour: Color },
    /// A flat ring on the ground for `secs`.
    AmmoRing { at: Vec3, radius: f32, colour: Color, secs: f32 },
    /// A Proc's sphere or ring for `secs`.
    Proc { at: Vec3, radius: f32, colour: Color, secs: f32, sphere: bool },
    /// Ricochet's tracer.
    Tracer { from: Vec3, to: Vec3, colour: Color, secs: f32 },
    /// Cleave's sweep.
    Cleave { feet: Vec3, forward: Vec3, radius: f32, colour: Color, secs: f32 },
    /// Jackpot's burst.
    Jackpot { at: Vec3, radius: f32, colour: Color, secs: f32 },
    /// Scorched's disc.
    Scorched { at: Vec3, radius: f32, colour: Color, secs: f32 },
    /// Cleave's Frost patch, which fades over `secs`.
    Ring { at: Vec3, secs: f32 },
    /// Supply Drop's shell, flown from `from` to `to`.
    Arc { from: Vec3, to: Vec3 },
}

/// An `Fx` as it goes on the wire: positions as `pack_position` triples, a radius in decimetres,
/// a colour as one byte (3 bits red, 3 green, 2 blue) and a life in 10 ms steps.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxWire {
    AmmoLine { from: [u8; 6], to: [u8; 6], colour: u8 },
    AmmoBurst { at: [u8; 6], radius: u8, colour: u8 },
    AmmoRing { at: [u8; 6], radius: u8, colour: u8, life: u8 },
    Proc { at: [u8; 6], radius: u8, colour: u8, life: u8, sphere: bool },
    Tracer { from: [u8; 6], to: [u8; 6], colour: u8, life: u8 },
    Cleave { feet: [u8; 6], forward: [i8; 2], radius: u8, colour: u8, life: u8 },
    Jackpot { at: [u8; 6], radius: u8, colour: u8, life: u8 },
    Scorched { at: [u8; 6], radius: u8, colour: u8, life: u8 },
    Ring { at: [u8; 6], life: u8 },
    Arc { from: [u8; 6], to: [u8; 6] },
}

/// A colour as one byte.
pub fn pack_colour(colour: Color) -> u8 {
    let colour = colour.to_srgba();
    let level = |value: f32, top: f32| (value.clamp(0.0, 1.0) * top).round() as u8;
    (level(colour.red, 7.0) << 5) | (level(colour.green, 7.0) << 2) | level(colour.blue, 3.0)
}

/// The colour a byte stands for.
pub fn unpack_colour(byte: u8) -> Color {
    Color::srgb(f32::from(byte >> 5) / 7.0, f32::from((byte >> 2) & 7) / 7.0, f32::from(byte & 3) / 3.0)
}

fn pack_radius(radius: f32) -> u8 {
    (radius * 10.0).round().clamp(0.0, 255.0) as u8
}

fn unpack_radius(byte: u8) -> f32 {
    f32::from(byte) / 10.0
}

fn pack_life(secs: f32) -> u8 {
    (secs * 100.0).round().clamp(0.0, 255.0) as u8
}

fn unpack_life(byte: u8) -> f32 {
    f32::from(byte) / 100.0
}

fn pack_direction(forward: Vec3) -> [i8; 2] {
    let flat = forward.with_y(0.0).normalize_or_zero();
    [(flat.x * 127.0).round() as i8, (flat.z * 127.0).round() as i8]
}

fn unpack_direction([x, z]: [i8; 2]) -> Vec3 {
    Vec3::new(f32::from(x), 0.0, f32::from(z)).normalize_or_zero()
}

impl Fx {
    /// This as it goes over the wire, positions counted from `origin`.
    pub fn pack(self, origin: Vec3) -> FxWire {
        let at = |position: Vec3| pack_position(origin, position);
        match self {
            Fx::AmmoLine { from, to, colour } => FxWire::AmmoLine { from: at(from), to: at(to), colour: pack_colour(colour) },
            Fx::AmmoBurst { at: place, radius, colour } => {
                FxWire::AmmoBurst { at: at(place), radius: pack_radius(radius), colour: pack_colour(colour) }
            }
            Fx::AmmoRing { at: place, radius, colour, secs } => FxWire::AmmoRing {
                at: at(place),
                radius: pack_radius(radius),
                colour: pack_colour(colour),
                life: pack_life(secs),
            },
            Fx::Proc { at: place, radius, colour, secs, sphere } => FxWire::Proc {
                at: at(place),
                radius: pack_radius(radius),
                colour: pack_colour(colour),
                life: pack_life(secs),
                sphere,
            },
            Fx::Tracer { from, to, colour, secs } => {
                FxWire::Tracer { from: at(from), to: at(to), colour: pack_colour(colour), life: pack_life(secs) }
            }
            Fx::Cleave { feet, forward, radius, colour, secs } => FxWire::Cleave {
                feet: at(feet),
                forward: pack_direction(forward),
                radius: pack_radius(radius),
                colour: pack_colour(colour),
                life: pack_life(secs),
            },
            Fx::Jackpot { at: place, radius, colour, secs } => FxWire::Jackpot {
                at: at(place),
                radius: pack_radius(radius),
                colour: pack_colour(colour),
                life: pack_life(secs),
            },
            Fx::Scorched { at: place, radius, colour, secs } => FxWire::Scorched {
                at: at(place),
                radius: pack_radius(radius),
                colour: pack_colour(colour),
                life: pack_life(secs),
            },
            Fx::Ring { at: place, secs } => FxWire::Ring { at: at(place), life: pack_life(secs) },
            Fx::Arc { from, to } => FxWire::Arc { from: at(from), to: at(to) },
        }
    }
}

impl FxWire {
    /// What this says, positions counted from `origin`.
    pub fn unpack(self, origin: Vec3) -> Fx {
        let at = |bytes: [u8; 6]| unpack_position(origin, bytes);
        match self {
            FxWire::AmmoLine { from, to, colour } => Fx::AmmoLine { from: at(from), to: at(to), colour: unpack_colour(colour) },
            FxWire::AmmoBurst { at: place, radius, colour } => {
                Fx::AmmoBurst { at: at(place), radius: unpack_radius(radius), colour: unpack_colour(colour) }
            }
            FxWire::AmmoRing { at: place, radius, colour, life } => Fx::AmmoRing {
                at: at(place),
                radius: unpack_radius(radius),
                colour: unpack_colour(colour),
                secs: unpack_life(life),
            },
            FxWire::Proc { at: place, radius, colour, life, sphere } => Fx::Proc {
                at: at(place),
                radius: unpack_radius(radius),
                colour: unpack_colour(colour),
                secs: unpack_life(life),
                sphere,
            },
            FxWire::Tracer { from, to, colour, life } => {
                Fx::Tracer { from: at(from), to: at(to), colour: unpack_colour(colour), secs: unpack_life(life) }
            }
            FxWire::Cleave { feet, forward, radius, colour, life } => Fx::Cleave {
                feet: at(feet),
                forward: unpack_direction(forward),
                radius: unpack_radius(radius),
                colour: unpack_colour(colour),
                secs: unpack_life(life),
            },
            FxWire::Jackpot { at: place, radius, colour, life } => Fx::Jackpot {
                at: at(place),
                radius: unpack_radius(radius),
                colour: unpack_colour(colour),
                secs: unpack_life(life),
            },
            FxWire::Scorched { at: place, radius, colour, life } => Fx::Scorched {
                at: at(place),
                radius: unpack_radius(radius),
                colour: unpack_colour(colour),
                secs: unpack_life(life),
            },
            FxWire::Ring { at: place, life } => Fx::Ring { at: at(place), secs: unpack_life(life) },
            FxWire::Arc { from, to } => Fx::Arc { from: at(from), to: at(to) },
        }
    }
}

/// The frame's effects as they go out: at most `NET_FX_MAX_PER_FRAME`, the oldest kept.
pub fn batch(frame: Vec<Fx>, origin: Vec3) -> Vec<FxWire> {
    frame.into_iter().take(NET_FX_MAX_PER_FRAME).map(|fx| fx.pack(origin)).collect()
}

/// Tells every Player what the Host drew this frame. With no one to tell, the records are only
/// thrown away.
pub fn send_fx(
    map: Res<MapId>,
    roster: Option<Res<Roster>>,
    wire: Option<ResMut<Wire>>,
    ammo: Option<ResMut<Flashes>>,
    procs: Option<ResMut<ProcFlashes>>,
    sets: Option<ResMut<SetFlashes>>,
) {
    let mut frame = Vec::new();
    if let Some(mut ammo) = ammo {
        frame.append(&mut ammo.sent);
    }
    if let Some(mut procs) = procs {
        frame.append(&mut procs.sent);
    }
    if let Some(mut sets) = sets {
        frame.append(&mut sets.sent);
    }
    let (Some(roster), Some(mut wire)) = (roster, wire) else { return };
    if frame.is_empty() || roster.0.is_empty() {
        return;
    }
    let bytes = encode(&ToPlayer::Fx(batch(frame, wire_origin(*map))));
    for &(peer, _) in &roster.0 {
        wire.0.send(peer, bytes.clone());
    }
}

/// Puts what the Host drew into this machine's own resources, which draw it.
pub fn hear_fx(
    mut commands: Commands,
    map: Res<MapId>,
    mut hits: ResMut<Hits>,
    mut ammo: Option<ResMut<Flashes>>,
    mut procs: Option<ResMut<ProcFlashes>>,
    mut sets: Option<ResMut<SetFlashes>>,
) {
    let origin = wire_origin(*map);
    for fx in hits.take_fx() {
        let fx = fx.unpack(origin);
        match fx {
            Fx::AmmoLine { .. } | Fx::AmmoBurst { .. } | Fx::AmmoRing { .. } => {
                if let Some(ammo) = ammo.as_mut() {
                    ammo.hear(fx);
                }
            }
            Fx::Proc { .. } => {
                if let Some(procs) = procs.as_mut() {
                    procs.hear(fx);
                }
            }
            Fx::Tracer { .. } | Fx::Cleave { .. } | Fx::Jackpot { .. } | Fx::Scorched { .. } => {
                if let Some(sets) = sets.as_mut() {
                    sets.hear(fx);
                }
            }
            Fx::Ring { at, secs } => {
                commands.spawn((crate::state::InRun, Name::new("Cleave trail"), FrostPatch::fading(at, secs)));
            }
            Fx::Arc { from, to } => {
                commands.spawn((crate::state::InRun, Name::new("Supply Drop frag"), ShellView::new(from, to)));
            }
        }
    }
}

/// Puts the systems in the app. The Host's run only while the Roster is there, the client's while
/// the Prediction is.
pub fn add(app: &mut App) {
    app.add_systems(Update, send_fx.in_set(HostSimulation).run_if(world_runs).run_if(resource_exists::<MapId>))
        .add_systems(Update, hear_fx.run_if(world_runs).run_if(resource_exists::<Prediction>).run_if(resource_exists::<MapId>));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_kind() -> Vec<Fx> {
        let at = Vec3::new(3.25, 1.5, -7.5);
        let colour = Color::srgb(1.0, 0.0, 0.0);
        vec![
            Fx::AmmoLine { from: at, to: at + Vec3::X, colour },
            Fx::AmmoBurst { at, radius: 2.0, colour },
            Fx::AmmoRing { at, radius: 1.5, colour, secs: 0.25 },
            Fx::Proc { at, radius: 3.0, colour, secs: 0.5, sphere: true },
            Fx::Tracer { from: at, to: at + Vec3::Z, colour, secs: 0.3 },
            Fx::Cleave { feet: at, forward: Vec3::X, radius: 2.0, colour, secs: 0.3 },
            Fx::Jackpot { at, radius: 1.2, colour, secs: 0.6 },
            Fx::Scorched { at, radius: 2.5, colour, secs: 2.0 },
            Fx::Ring { at, secs: 2.0 },
            Fx::Arc { from: at, to: at + Vec3::new(8.0, 0.0, 2.0) },
        ]
    }

    #[test]
    fn every_effect_survives_the_wire_to_within_a_centimetre() {
        for fx in every_kind() {
            let wire = fx.pack(Vec3::ZERO);
            let bytes = encode(&wire);
            let back: FxWire = super::super::protocol::decode(&bytes).unwrap();
            assert_eq!(back, wire);
            let again = back.unpack(Vec3::ZERO);
            assert_eq!(std::mem::discriminant(&again), std::mem::discriminant(&fx));
            assert!(bytes.len() <= 20, "{fx:?} took {} bytes", bytes.len());
        }
    }

    #[test]
    fn a_pure_primary_colour_goes_through_a_byte_unchanged() {
        for colour in [Color::srgb(1.0, 0.0, 0.0), Color::srgb(0.0, 1.0, 0.0), Color::srgb(0.0, 0.0, 1.0), Color::WHITE] {
            let (a, b) = (unpack_colour(pack_colour(colour)).to_srgba(), colour.to_srgba());
            assert!((a.red - b.red).abs() + (a.green - b.green).abs() + (a.blue - b.blue).abs() < 1e-5);
        }
    }

    #[test]
    fn over_the_per_frame_cap_the_newest_effects_are_dropped() {
        let frame: Vec<Fx> = (0..NET_FX_MAX_PER_FRAME + 10)
            .map(|i| Fx::AmmoBurst { at: Vec3::new(i as f32, 0.0, 0.0), radius: 1.0, colour: Color::WHITE })
            .collect();
        let sent = batch(frame, Vec3::ZERO);
        assert_eq!(sent.len(), NET_FX_MAX_PER_FRAME);
        let Fx::AmmoBurst { at, .. } = sent.last().unwrap().unpack(Vec3::ZERO) else { panic!() };
        assert_eq!(at.x, (NET_FX_MAX_PER_FRAME - 1) as f32);
    }
}
