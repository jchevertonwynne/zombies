//! Jump Pads: steel plates on the floor that, once the Power is on, throw a Player who steps onto
//! one up to a higher floor. The throw is part of the movement step, so a client replaying its
//! inputs throws the same way the Host does. Zombies never use them.

use bevy::prelude::*;

use crate::maps::map::MapId;
use crate::tuning::*;
use crate::maps::zone::ZoneId;

/// How far feet can be off the plate's top, up or down, and still stand on it: a ground probe's
/// worth of slack either way.
const ON_PLATE_SLACK: f32 = 0.15;

/// A Jump Pad: a plate on the floor of `zone`, centred on `at`, that once the Power is on throws
/// a Player who steps onto it up to land at `to`, on a higher floor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JumpPad {
    pub zone: ZoneId,
    pub at: Vec3,
    pub to: Vec3,
}

impl JumpPad {
    /// Whether feet at `feet` stand on the plate.
    pub fn under(&self, feet: Vec3) -> bool {
        let across = Vec2::new(feet.x - self.at.x, feet.z - self.at.z).length();
        let up = feet.y - self.at.y;
        across <= JUMP_PAD_RADIUS && (-ON_PLATE_SLACK..=JUMP_PAD_PLATE + ON_PLATE_SLACK).contains(&up)
    }
}

/// The Jump Pads that throw on `map` now: all of them once the Power is on, none before.
pub fn working(map: MapId, power_on: bool) -> &'static [JumpPad] {
    if power_on { map.layout().jump_pads } else { &[] }
}

/// The velocity that throws feet at `from` in an arc peaking `JUMP_PAD_APEX` over the higher of
/// its two ends and coming down at `to`, under `GRAVITY` and with nothing steering it.
pub fn throw(from: Vec3, to: Vec3) -> Vec3 {
    let apex = from.y.max(to.y) + JUMP_PAD_APEX;
    let up = (2.0 * GRAVITY * (apex - from.y)).sqrt();
    let flight = up / GRAVITY + (2.0 * (apex - to.y) / GRAVITY).sqrt();
    let across = Vec3::new(to.x - from.x, 0.0, to.z - from.z) / flight;
    across + Vec3::Y * up
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where a body thrown from `from` at `velocity` is after `secs`, under gravity alone.
    fn flight(from: Vec3, velocity: Vec3, secs: f32) -> Vec3 {
        from + velocity * secs + Vec3::NEG_Y * GRAVITY * secs * secs / 2.0
    }

    /// Seconds until a body thrown upward at `vy` from `from_y` comes back down through `to_y`.
    fn coming_down_through(from_y: f32, vy: f32, to_y: f32) -> f32 {
        // from_y + vy t - g t^2 / 2 = to_y, the later root.
        let (a, b, c) = (-GRAVITY / 2.0, vy, from_y - to_y);
        (-b - (b * b - 4.0 * a * c).sqrt()) / (2.0 * a)
    }

    #[test]
    fn a_throw_comes_down_on_its_landing_spot() {
        let (from, to) = (Vec3::new(6.0, 0.0, -3.0), Vec3::new(6.0, 4.0, 7.0));
        let velocity = throw(from, to);
        let secs = coming_down_through(from.y, velocity.y, to.y);
        assert!(flight(from, velocity, secs).distance(to) < 0.01, "lands at {}", flight(from, velocity, secs));
    }

    #[test]
    fn a_throw_peaks_well_over_the_floor_it_lands_on() {
        let (from, to) = (Vec3::new(0.0, 8.0, 0.0), Vec3::new(3.0, 12.0, -6.0));
        let velocity = throw(from, to);
        let apex = from.y + velocity.y * velocity.y / (2.0 * GRAVITY);
        assert!((apex - (12.0 + JUMP_PAD_APEX)).abs() < 0.01, "peaks at {apex}");
    }

    #[test]
    fn a_player_stands_on_the_plate_only_within_its_rim() {
        let pad = JumpPad { zone: ZoneId::QuarryFloor, at: Vec3::new(10.0, 0.0, 10.0), to: Vec3::new(10.0, 4.0, 20.0) };
        assert!(pad.under(Vec3::new(10.0, JUMP_PAD_PLATE, 10.0)));
        assert!(pad.under(Vec3::new(10.0 + JUMP_PAD_RADIUS * 0.9, JUMP_PAD_PLATE, 10.0)));
        assert!(!pad.under(Vec3::new(10.0 + JUMP_PAD_RADIUS + 0.1, 0.0, 10.0)), "beside it");
        assert!(!pad.under(Vec3::new(10.0, 1.5, 10.0)), "in the air over it");
    }
}
