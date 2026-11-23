//! The Sanatorium's fluorescent tubes flicker (#522). Each Tube lamp carries a `LampFlicker`, a
//! factor `light_lamps` multiplies into its strength (and the tube's glow follows):
//!
//! - Before the Power (Dormant) every lamp runs at `DORMANT_LEVEL`, and one in three stutters.
//! - Once the Power is on every lamp is steady at full but one in six, picked by a hash of its
//!   position (so the same lamps every Run, on every machine), never in the Operating Theatre.
//!   Every few seconds that lamp gives a burst of 2 to 5 short dips.
//!
//! The Quarantine's stages (Warning, Sealed, Release) are the Quarantine look ticket's, and add
//! to `LampFlicker::factor`. Only the Power, which every Peer already has, decides anything here:
//! the idle flicker's timing is local (its phase isn't synced) and is no gameplay fact.

use bevy::prelude::*;

use crate::maps::zone::ZoneId;

/// How lit the lamps are before the Power.
pub const DORMANT_LEVEL: f32 = 0.6;
/// What a dip drops a lamp to, as a fraction of full.
pub const DIP_LEVEL: f32 = 0.15;
/// A burst is this many dips, and each dip (and each gap between them) lasts this many seconds.
const DIPS: std::ops::RangeInclusive<u32> = 2..=5;
const DIP_SECS: std::ops::RangeInclusive<f32> = 0.04..=0.12;
/// A stuttering Dormant lamp bursts this often, and an idle one this often.
const DORMANT_EVERY_SECS: std::ops::RangeInclusive<f32> = 1.0..=4.0;
const IDLE_EVERY_SECS: std::ops::RangeInclusive<f32> = 3.0..=11.0;

/// A Tube lamp's burst of dips began: for the sound ticket's buzz and click.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct LampBurst {
    pub zone: ZoneId,
    pub at: Vec3,
}

/// Where a lamp is in its bursts.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Burst {
    /// Steady until `until`.
    Waiting { until: f32 },
    /// Dipped until `until`, with `left` dips after this one.
    Dipping { until: f32, left: u32 },
    /// Back to steady between dips until `until`, with `left` dips to come.
    Between { until: f32, left: u32 },
}

/// A Tube lamp's flicker, and its glowing tube.
#[derive(Component, Debug, Clone)]
pub struct LampFlicker {
    zone: ZoneId,
    at: Vec3,
    /// Whether it stutters while Dormant, and whether it bursts once the Power is on.
    stutters: bool,
    bursts: bool,
    burst: Burst,
    /// Whether the last frame was Powered, so the schedule restarts on the Power.
    powered: Option<bool>,
    rng: u32,
    /// The factor last applied, so the tube's glow is written only on change.
    pub(super) applied: f32,
    /// The tube's material and its emissive colour at full.
    pub(super) glow: Option<(Handle<StandardMaterial>, LinearRgba)>,
}

/// A stable hash of a lamp's position, the same on every machine and every Run.
pub fn position_hash(at: Vec3) -> u32 {
    let mut hash = 0x811c_9dc5_u32;
    for part in [at.x, at.y, at.z] {
        for byte in part.to_bits().to_le_bytes() {
            hash = (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193);
        }
    }
    // Finish, so the low bits (which pick) are well mixed.
    hash ^= hash >> 15;
    hash = hash.wrapping_mul(0x2c1b_3c6d);
    hash ^= hash >> 12;
    hash = hash.wrapping_mul(0x297a_2d39);
    hash ^ (hash >> 15)
}

impl LampFlicker {
    pub fn new(zone: ZoneId, at: Vec3, glow: Option<(Handle<StandardMaterial>, LinearRgba)>) -> Self {
        let hash = position_hash(at);
        Self {
            zone,
            at,
            stutters: (hash >> 8).is_multiple_of(3),
            bursts: hash.is_multiple_of(6) && zone != ZoneId::OperatingTheatre,
            burst: Burst::Waiting { until: 0.0 },
            powered: None,
            rng: hash | 1,
            applied: f32::NAN,
            glow,
        }
    }

    #[cfg(test)]
    /// Whether this lamp is one of the few that flicker once the Power is on.
    pub fn flickers_when_idle(&self) -> bool {
        self.bursts
    }

    #[cfg(test)]
    /// Whether this lamp stutters before the Power.
    pub fn stutters_when_dormant(&self) -> bool {
        self.stutters
    }

    fn roll(&mut self) -> u32 {
        // xorshift32
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }

    fn next(&mut self, range: &std::ops::RangeInclusive<f32>) -> f32 {
        let unit = (self.roll() >> 8) as f32 / (1u32 << 24) as f32;
        range.start() + (range.end() - range.start()) * unit
    }

    /// The factor on the lamp's light at `now` seconds, from `DIP_LEVEL` to 1, and whether a burst
    /// began just now. Steady at `DORMANT_LEVEL` before the Power and 1 after, but for a burst.
    pub fn factor(&mut self, now: f32, powered: bool) -> (f32, bool) {
        let steady = if powered { 1.0 } else { DORMANT_LEVEL };
        let active = if powered { self.bursts } else { self.stutters };
        if !active {
            return (steady, false);
        }
        let every = if powered { IDLE_EVERY_SECS } else { DORMANT_EVERY_SECS };
        if self.powered != Some(powered) {
            // The Power changes everything at once: start the new schedule.
            self.powered = Some(powered);
            let wait = self.next(&every);
            self.burst = Burst::Waiting { until: now + wait };
        }
        let mut began = false;
        loop {
            match self.burst {
                Burst::Waiting { until } if now >= until => {
                    let left = self.dips();
                    let dip = self.next(&DIP_SECS);
                    self.burst = Burst::Dipping { until: until + dip, left };
                    began = true;
                }
                Burst::Dipping { until, left } if now >= until => {
                    if left == 0 {
                        let wait = self.next(&every);
                        self.burst = Burst::Waiting { until: until + wait };
                    } else {
                        let gap = self.next(&DIP_SECS);
                        self.burst = Burst::Between { until: until + gap, left };
                    }
                }
                Burst::Between { until, left } if now >= until => {
                    let dip = self.next(&DIP_SECS);
                    self.burst = Burst::Dipping { until: until + dip, left: left - 1 };
                }
                _ => break,
            }
        }
        let level = if matches!(self.burst, Burst::Dipping { .. }) { DIP_LEVEL } else { steady };
        (level, began)
    }

    /// How many dips a burst has after its first: 1 to 4, so 2 to 5 in all.
    fn dips(&mut self) -> u32 {
        let (lo, hi) = (*DIPS.start(), *DIPS.end());
        lo + self.roll() % (hi - lo + 1) - 1
    }

    pub fn burst_event(&self) -> LampBurst {
        LampBurst { zone: self.zone, at: self.at }
    }
}
