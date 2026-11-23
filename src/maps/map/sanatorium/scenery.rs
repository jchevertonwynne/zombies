//! The Sanatorium's look in code (#522): the emergency bulkheads, the dead theatre lamp over
//! Pack-a-Punch, and its outside (the fogbound hill's apron, dead trees, the Boiler House chimney
//! and the stair tower's roof tank). All of it is built from the Layout on every Peer, so a client
//! sees the same Map; none of it has a collider.

use bevy::prelude::*;

use super::{FIRST, H, TALL, palette};
use crate::maps::map::MapId;
use crate::maps::map::build::Builder;
use crate::maps::map::textures::Material;
use crate::maps::zone::ZoneId;
use crate::weapons::pack_a_punch::machine_at;

/// An emergency bulkhead: a small box with a white-green glow and no light source. The Quarantine
/// look ticket turns a Wing's red while it is Sealed.
#[derive(Component, Debug, Clone, Copy)]
#[allow(dead_code)] // The Quarantine look ticket reads the Zone.
pub struct Bulkhead(pub ZoneId);

/// The bulkheads (12): two in each corridor, one at each stair's foot, and one in each Wing Zone.
/// Each is a Zone, the centre of its box on a wall's face, and whether that wall runs along z.
pub(super) const BULKHEADS: [(ZoneId, Vec3, bool); 12] = {
    use ZoneId::*;
    [
        (WestCorridor, Vec3::new(-46.0, 2.6, 8.24), false),
        (WestCorridor, Vec3::new(-13.0, 2.6, 8.24), false),
        (EastCorridor, Vec3::new(13.0, 2.6, 8.24), false),
        (EastCorridor, Vec3::new(46.0, 2.6, 8.24), false),
        // The Main Stair's tower and the Fire Escape's bay.
        (UpperLanding, Vec3::new(-3.76, 2.4, 60.0), true),
        (GardenCourt, Vec3::new(-28.76, 2.4, 15.0), true),
        (LongWard, Vec3::new(-49.76, 2.6, 30.0), true),
        (DayRoom, Vec3::new(-30.0, 2.6, 55.76), false),
        (BathHall, Vec3::new(49.76, 2.6, 27.0), true),
        (TreatmentRoom, Vec3::new(30.0, 2.6, 55.76), false),
        (IsolationWard, Vec3::new(-18.0, FIRST + 2.6, 42.24), false),
        (ChildrensWard, Vec3::new(-30.0, FIRST + 2.6, 42.24), false),
    ]
};

/// The apron's slope, rise over run: it falls 1 in 4 to z -70.
const APRON_FALL: f32 = 0.25;
const APRON_START: f32 = -30.0;
const APRON_END: f32 = -70.0;
const APRON_HALF_WIDTH: f32 = 90.0;

/// The ground of the hill at `z` on the apron.
fn apron_height(z: f32) -> f32 {
    (z - APRON_START) * APRON_FALL
}

/// The look's pieces, built after the walls.
pub(super) fn build(b: &mut Builder) {
    let paint = palette(None);
    for &(zone, at, along_z) in &BULKHEADS {
        let size = if along_z { Vec3::new(0.08, 0.15, 0.4) } else { Vec3::new(0.4, 0.15, 0.08) };
        let bulkhead = b.glow_box(size, at, Color::srgb(0.7, 1.0, 0.8), 4.0);
        b.tag(bulkhead, Bulkhead(zone));
    }

    // The dead lamp over Pack-a-Punch: a 1.2 m disc housing on an arm from the ceiling, unlit.
    let machine = machine_at(MapId::Sanatorium);
    let ceiling = FIRST + H;
    let housing = palette(Some(ZoneId::OperatingTheatre)).trim;
    b.drum(Vec3::new(1.2, 0.12, 1.2), Vec3::new(machine.x, ceiling - 0.7, machine.z), housing);
    b.drum(Vec3::new(1.0, 0.03, 1.0), Vec3::new(machine.x, ceiling - 0.78, machine.z), Color::srgb(0.1, 0.11, 0.12));
    b.shape(Vec3::new(0.12, 0.64, 0.12), Vec3::new(machine.x, ceiling - 0.32, machine.z), Quat::IDENTITY, housing);

    // The hill: an apron of grass falling away past the south railings, to z -70 and x +-90. Only
    // drawn: it lies past the plan, which is Out of Bounds anyway.
    let run = APRON_START - APRON_END;
    let length = (run * run * (1.0 + APRON_FALL * APRON_FALL)).sqrt();
    let tilt = Quat::from_rotation_x(-APRON_FALL.atan());
    let thick = 0.4;
    let middle = Vec3::new(0.0, apron_height((APRON_START + APRON_END) / 2.0), (APRON_START + APRON_END) / 2.0);
    let normal = tilt * Vec3::Y;
    b.shape_in(Vec3::new(2.0 * APRON_HALF_WIDTH, thick, length), middle - normal * thick / 2.0, tilt, paint.floor, Some(Material::GrassGround));

    // Four dead trees on it, silhouettes: a trunk and three bare limbs.
    let bark = Color::srgb(0.045, 0.05, 0.045);
    for (x, z, lean) in [(-34.0, -42.0, 0.1), (-9.0, -57.0, -0.15), (21.0, -47.0, 0.2), (47.0, -64.0, -0.08)] {
        let foot = Vec3::new(x, apron_height(z) - 0.2, z);
        b.shape(Vec3::new(0.4, 6.0, 0.4), foot + Vec3::Y * 3.0, Quat::from_rotation_z(lean), bark);
        for (up, spread, turn) in [(3.8, 0.9, 0.0), (4.6, -0.8, 1.6), (5.4, 0.6, 3.2)] {
            let rotation = Quat::from_rotation_y(turn) * Quat::from_rotation_z(spread);
            b.shape(Vec3::new(0.18, 2.6, 0.18), foot + Vec3::Y * up + rotation * Vec3::Y * 1.2, rotation, bark);
        }
    }

    // The Boiler House's chimney: a 1.8 m square brick stack to y 20 above its roof, the
    // landmark from both courts and the Sun Roof, with a cap in the Boiler House's trim.
    let boiler = palette(Some(ZoneId::BoilerHouse));
    b.shape_in(Vec3::new(1.8, 20.0 - TALL, 1.8), Vec3::new(-61.0, (20.0 + TALL) / 2.0, 4.0), Quat::IDENTITY, boiler.wall, Some(Material::BrickWall001));
    b.shape(Vec3::new(2.2, 0.3, 2.2), Vec3::new(-61.0, 20.15, 4.0), Quat::IDENTITY, boiler.trim);

    // The stair tower's flat roof, with a coping in trim paint and a water tank on legs.
    let roof = TALL + 0.2;
    let landing = palette(Some(ZoneId::UpperLanding));
    for (dx, dz) in [(-0.9, -0.9), (0.9, -0.9), (-0.9, 0.9), (0.9, 0.9)] {
        b.shape(Vec3::new(0.2, 0.6, 0.2), Vec3::new(dx, roof + 0.3, 60.0 + dz), Quat::IDENTITY, landing.trim);
    }
    b.shape_in(Vec3::new(2.4, 1.6, 2.4), Vec3::new(0.0, roof + 1.4, 60.0), Quat::IDENTITY, Color::srgb(0.45, 0.3, 0.2), Some(Material::RustyMetal02));
    b.shape(Vec3::new(8.4, 0.25, 0.4), Vec3::new(0.0, roof + 0.125, 64.0 - 0.2), Quat::IDENTITY, landing.trim);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_twelve_bulkheads_two_to_a_corridor_one_to_a_wing_zone() {
        assert_eq!(BULKHEADS.len(), 12);
        let in_zone = |zone| BULKHEADS.iter().filter(|(z, ..)| *z == zone).count();
        assert_eq!((in_zone(ZoneId::WestCorridor), in_zone(ZoneId::EastCorridor)), (2, 2));
        for zone in super::super::WINGS.iter().flat_map(|wing| wing.zones) {
            assert_eq!(in_zone(*zone), 1, "{zone:?}");
        }
    }

    #[test]
    fn the_apron_falls_one_in_four_to_z_minus_70() {
        assert_eq!(apron_height(APRON_START), 0.0);
        assert_eq!(apron_height(APRON_END), -10.0);
    }
}
