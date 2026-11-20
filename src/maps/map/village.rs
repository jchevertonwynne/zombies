//! The Village Map: a long village laid out like a ladder, flat and mostly out of doors. Two long
//! ways run east and west: the High Street to the south (the West End, the Market in the middle,
//! and the East End, each cut off from the next by a barricade) and the lanes behind to the north
//! (Mill Lane, the Back Lane and Church Lane). Between them stand the village's buildings, each
//! with a Door onto both: the Mill, the Smithy, the Bakery, the Inn, the Schoolhouse and the
//! Church. Off the lanes' far ends lie the Orchard and the Graveyard, open ground full of Risers.
//!
//! A Run starts in the Inn, in the middle of the village. The Power Switch is in the Mill at the
//! far west end, and Pack-a-Punch stands on the Church's chancel at the far east end, so the
//! village is crossed end to end. The Back Lane is gated off from the lanes either side by Power
//! Doors: before the Power, the north side is a row of dead ends; once it's on, the lanes join up
//! and the whole village becomes one long ring, with a shortcut through every building.
//!
//! The whole Map is centred on the world's origin along x (ADR 0009).

use bevy::prelude::*;

use crate::enemies::nav::Gate;
use crate::enemies::sapper::SapperTiming;
use crate::maps::map::BarrierId;
use crate::maps::zone::ZoneId;
use crate::tuning::*;
use crate::weapons::arsenal::WeaponId;

use super::build::{Builder, Gap};
use super::{DOOR_HEIGHT, Infill, Layout, Prop, PropKind, RaisedFloor, STOREY, Slope, door, infill, palette, prop};

/// The first of the Village's windows in `WINDOWS`, after the Greybox's and the Quarry's.
const W0: u8 = (super::greybox::WINDOWS.len() + super::quarry::WINDOWS.len()) as u8;

/// How high a chalk outline's centre sits on its wall.
const CHALK: f32 = 1.5;

/// A point in the Village, `x` along from its middle.
const fn at(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

/// A spot on a wall or a floor's plan in the Village, `x` along from its middle.
const fn v(x: f32, z: f32) -> Vec2 {
    Vec2::new(x, z)
}

const fn barrier(i: u8) -> Gate {
    Gate::Barrier(BarrierId(W0 + i))
}

pub(super) const VILLAGE: Layout = Layout {
    zones: &{
        use ZoneId::*;
        [Inn, Market, BackLane, Bakery, WestEnd, EastEnd, Smithy, Mill, Schoolhouse, Church, MillLane, ChurchLane, Orchard, Graveyard]
    },
    // In the middle of the Inn, clear of its tables, its bar and the Doors.
    seats: [at(0.0, 0.0, 14.0), at(-3.0, 0.0, 14.0), at(3.0, 0.0, 14.0), at(0.0, 0.0, 17.0)],
    wall_weapons: &WALL_WEAPONS,
    windows: &WINDOWS,
    risers: &RISERS,
    wading: &[],
    hellhound_spawns: &HELLHOUND_SPAWNS,
    special_spawns: &SPECIAL_SPAWNS,
    vents: &VENTS,
    raised_floors: &[CHANCEL],
    infills: &INFILLS,
    slopes: &SLOPES,
    sunken: &[],
    water: &[],
    canals: &[],
    props: PROPS,
    lamps: &LAMPS,
    fixed_joins: &FIXED_JOINS,
    shifting: &[],
    nav_nodes: NODES,
    nav_edges: EDGES,
    jump_pads: &[],
    ziplines: &[],
    sapper: SapperTiming::DEFAULT,
    sections: &[],
    generators: &[],
    darkness: false,
    outdoor: &[ZoneId::Market, ZoneId::WestEnd, ZoneId::EastEnd, ZoneId::BackLane, ZoneId::MillLane, ZoneId::ChurchLane, ZoneId::Orchard, ZoneId::Graveyard],
    roofed: true,
    outside: Some(super::Outside { floor: super::textures::Material::LeafyGrass, wall: super::textures::Material::PlasteredStoneWall }),
    sun: Vec3::new(-1.0, -3.0, 1.5),
    look: crate::maps::look::VILLAGE_NIGHT,
    plan: (Vec2::new(-80.0, -30.0), Vec2::new(80.0, 60.0)),
    build,
};

/// The chalk outlines: the cheap guns in the Inn, an SMG on the Market and the dearer ones out
/// along the High Street.
const WALL_WEAPONS: [(WeaponId, ZoneId, Vec3, Vec2); 5] = [
    // Either side of the Inn's Door to the Market, facing the Players at the start.
    (WeaponId::M14, ZoneId::Inn, at(-4.5, CHALK, 6.2), Vec2::Y),
    (WeaponId::Olympia, ZoneId::Inn, at(4.5, CHALK, 6.2), Vec2::Y),
    // On the Bakery's front, across the Market from the cross.
    (WeaponId::Mp40, ZoneId::Market, at(-10.0, CHALK, 5.8), Vec2::NEG_Y),
    // On the south side of the street at each end.
    (WeaponId::TrenchGun, ZoneId::WestEnd, at(-48.0, CHALK, -5.8), Vec2::Y),
    (WeaponId::Ak74u, ZoneId::EastEnd, at(48.0, CHALK, -5.8), Vec2::Y),
];

/// The Box Locations, in order. Every Run starts the Box at the first, in the Bakery, two Doors
/// from the Inn either way round.
pub(super) const BOX_LOCATIONS: [(ZoneId, Vec3, Vec2); 7] = [
    // Against the Bakery's wall to the Inn.
    (ZoneId::Bakery, at(-8.65, 0.0, 14.0), Vec2::NEG_X),
    // Against the south side of the West End.
    (ZoneId::WestEnd, at(-54.0, 0.0, -5.35), Vec2::Y),
    // Against the Orchard's north wall.
    (ZoneId::Orchard, at(-64.0, 0.0, 53.35), Vec2::NEG_Y),
    // In the Schoolhouse, by the Door from the East End.
    (ZoneId::Schoolhouse, at(27.0, 0.0, 6.65), Vec2::Y),
    // Against the Back Lane's north wall.
    (ZoneId::BackLane, at(-6.0, 0.0, 29.35), Vec2::NEG_Y),
    // Against Church Lane's north wall.
    (ZoneId::ChurchLane, at(44.0, 0.0, 29.35), Vec2::NEG_Y),
    // Against the Market's south wall.
    (ZoneId::Market, at(-18.0, 0.0, -21.35), Vec2::Y),
];

/// The 32 windows. The buildings' are in their side walls, onto the walled gardens between them;
/// the streets' and lanes' are gaps in the walls round the village, onto the fields to the south
/// and the river bank to the north.
pub(super) const WINDOWS: [(ZoneId, Vec3, Vec2); 32] = [
    // 0-1: the Inn, onto the garden east of it.
    (ZoneId::Inn, at(8.0, 0.0, 10.0), Vec2::X),
    (ZoneId::Inn, at(8.0, 0.0, 18.0), Vec2::X),
    // 2-6: the Market: two onto the fields, one each side and one into the Inn's garden.
    (ZoneId::Market, at(-12.0, 0.0, -22.0), Vec2::NEG_Y),
    (ZoneId::Market, at(12.0, 0.0, -22.0), Vec2::NEG_Y),
    (ZoneId::Market, at(-24.0, 0.0, -14.0), Vec2::NEG_X),
    (ZoneId::Market, at(24.0, 0.0, -14.0), Vec2::X),
    (ZoneId::Market, at(16.0, 0.0, 6.0), Vec2::Y),
    // 7-12: the West End and the East End, onto the fields and off their far ends.
    (ZoneId::WestEnd, at(-60.0, 0.0, -6.0), Vec2::NEG_Y),
    (ZoneId::WestEnd, at(-36.0, 0.0, -6.0), Vec2::NEG_Y),
    (ZoneId::WestEnd, at(-72.0, 0.0, 0.0), Vec2::NEG_X),
    (ZoneId::EastEnd, at(36.0, 0.0, -6.0), Vec2::NEG_Y),
    (ZoneId::EastEnd, at(60.0, 0.0, -6.0), Vec2::NEG_Y),
    (ZoneId::EastEnd, at(72.0, 0.0, 0.0), Vec2::X),
    // 13-18: the buildings: the Mill's ends, the Smithy's, the Schoolhouse's and the Church's.
    (ZoneId::Mill, at(-72.0, 0.0, 14.0), Vec2::NEG_X),
    (ZoneId::Mill, at(-52.0, 0.0, 14.0), Vec2::X),
    (ZoneId::Smithy, at(-40.0, 0.0, 14.0), Vec2::NEG_X),
    (ZoneId::Schoolhouse, at(24.0, 0.0, 14.0), Vec2::NEG_X),
    (ZoneId::Schoolhouse, at(40.0, 0.0, 14.0), Vec2::X),
    (ZoneId::Church, at(52.0, 0.0, 14.0), Vec2::NEG_X),
    // 19-25: the lanes, onto the river bank, off their ends and into the Inn's garden.
    (ZoneId::BackLane, at(-12.0, 0.0, 30.0), Vec2::Y),
    (ZoneId::BackLane, at(12.0, 0.0, 30.0), Vec2::Y),
    (ZoneId::BackLane, at(16.0, 0.0, 22.0), Vec2::NEG_Y),
    (ZoneId::MillLane, at(-32.0, 0.0, 30.0), Vec2::Y),
    (ZoneId::MillLane, at(-72.0, 0.0, 26.0), Vec2::NEG_X),
    (ZoneId::ChurchLane, at(32.0, 0.0, 30.0), Vec2::Y),
    (ZoneId::ChurchLane, at(72.0, 0.0, 26.0), Vec2::X),
    // 26-31: the Orchard and the Graveyard, one in each of their outer walls.
    (ZoneId::Orchard, at(-72.0, 0.0, 42.0), Vec2::NEG_X),
    (ZoneId::Orchard, at(-56.0, 0.0, 54.0), Vec2::Y),
    (ZoneId::Orchard, at(-40.0, 0.0, 44.0), Vec2::X),
    (ZoneId::Graveyard, at(40.0, 0.0, 44.0), Vec2::NEG_X),
    (ZoneId::Graveyard, at(56.0, 0.0, 54.0), Vec2::Y),
    (ZoneId::Graveyard, at(72.0, 0.0, 42.0), Vec2::X),
];

/// The Risers: all out of doors, most of all among the graves.
const RISERS: [(ZoneId, Vec3); 18] = [
    (ZoneId::Market, at(-10.0, 0.0, -12.0)),
    (ZoneId::Market, at(10.0, 0.0, -12.0)),
    (ZoneId::Market, at(-6.0, 0.0, -18.0)),
    (ZoneId::Market, at(7.0, 0.0, 1.0)),
    (ZoneId::WestEnd, at(-56.0, 0.0, 2.0)),
    (ZoneId::WestEnd, at(-38.0, 0.0, -2.0)),
    (ZoneId::EastEnd, at(56.0, 0.0, 2.0)),
    (ZoneId::EastEnd, at(38.0, 0.0, -2.0)),
    (ZoneId::MillLane, at(-48.0, 0.0, 26.0)),
    (ZoneId::ChurchLane, at(52.0, 0.0, 26.0)),
    (ZoneId::Orchard, at(-62.0, 0.0, 42.0)),
    (ZoneId::Orchard, at(-54.0, 0.0, 37.5)),
    (ZoneId::Orchard, at(-46.0, 0.0, 49.0)),
    (ZoneId::Graveyard, at(50.0, 0.0, 33.0)),
    (ZoneId::Graveyard, at(62.0, 0.0, 33.0)),
    (ZoneId::Graveyard, at(49.5, 0.0, 50.0)),
    (ZoneId::Graveyard, at(62.5, 0.0, 50.0)),
    (ZoneId::Graveyard, at(56.0, 0.0, 50.5)),
];

/// The Hellhound Spawn Points, two in every Zone.
const HELLHOUND_SPAWNS: [(ZoneId, Vec3); 28] = [
    (ZoneId::Inn, at(5.0, 0.0, 9.0)),
    (ZoneId::Inn, at(2.5, 0.0, 19.5)),
    (ZoneId::Market, at(-10.0, 0.0, -6.0)),
    (ZoneId::Market, at(10.0, 0.0, -6.0)),
    (ZoneId::WestEnd, at(-56.0, 0.0, -2.0)),
    (ZoneId::WestEnd, at(-38.0, 0.0, 2.0)),
    (ZoneId::EastEnd, at(56.0, 0.0, -2.0)),
    (ZoneId::EastEnd, at(38.0, 0.0, 2.0)),
    (ZoneId::BackLane, at(-8.0, 0.0, 26.0)),
    (ZoneId::BackLane, at(8.0, 0.0, 26.0)),
    (ZoneId::MillLane, at(-44.0, 0.0, 26.0)),
    (ZoneId::MillLane, at(-66.0, 0.0, 26.0)),
    (ZoneId::ChurchLane, at(48.0, 0.0, 26.0)),
    (ZoneId::ChurchLane, at(66.0, 0.0, 26.0)),
    (ZoneId::Mill, at(-68.0, 0.0, 16.0)),
    (ZoneId::Mill, at(-56.0, 0.0, 16.0)),
    (ZoneId::Smithy, at(-35.0, 0.0, 11.0)),
    (ZoneId::Smithy, at(-29.0, 0.0, 18.0)),
    (ZoneId::Bakery, at(-19.0, 0.0, 18.0)),
    (ZoneId::Bakery, at(-12.0, 0.0, 10.0)),
    (ZoneId::Schoolhouse, at(32.0, 0.0, 12.0)),
    (ZoneId::Schoolhouse, at(32.0, 0.0, 19.0)),
    (ZoneId::Church, at(55.0, 0.0, 10.0)),
    (ZoneId::Church, at(55.0, 0.0, 18.0)),
    (ZoneId::Orchard, at(-62.0, 0.0, 33.0)),
    (ZoneId::Orchard, at(-50.0, 0.0, 51.0)),
    (ZoneId::Graveyard, at(51.0, 0.0, 33.0)),
    (ZoneId::Graveyard, at(61.0, 0.0, 51.5)),
];

/// The Special Spawn Points, one in each of the bigger Zones.
const SPECIAL_SPAWNS: [(ZoneId, Vec3); 8] = [
    (ZoneId::Market, at(-10.0, 0.0, -6.0)),
    (ZoneId::WestEnd, at(-56.0, 0.0, -2.0)),
    (ZoneId::EastEnd, at(56.0, 0.0, -2.0)),
    (ZoneId::BackLane, at(-8.0, 0.0, 26.0)),
    (ZoneId::Mill, at(-56.0, 0.0, 16.0)),
    (ZoneId::Church, at(55.0, 0.0, 10.0)),
    (ZoneId::Orchard, at(-62.0, 0.0, 33.0)),
    (ZoneId::Graveyard, at(51.0, 0.0, 33.0)),
];

/// The Vents, in the rafters of four of the buildings.
const VENTS: [(ZoneId, Vec3); 4] = [
    (ZoneId::Bakery, at(-18.0, 0.0, 14.0)),
    (ZoneId::Smithy, at(-32.0, 0.0, 14.0)),
    (ZoneId::Schoolhouse, at(32.0, 0.0, 14.0)),
    (ZoneId::Church, at(58.0, 0.0, 14.0)),
];

/// The gateways off the lanes into the Orchard and the Graveyard, always open.
const FIXED_JOINS: [(ZoneId, ZoneId); 2] = [(ZoneId::MillLane, ZoneId::Orchard), (ZoneId::ChurchLane, ZoneId::Graveyard)];

/// The lamps: four in each building, and gas lamps on posts down every street, lane and field, so
/// no point of any Zone's floor is further than `LAMP_REACH` from one in its Zone (a test checks).
const LAMPS: [(ZoneId, Vec3); 86] = [
    (ZoneId::Inn, at(-3.5, STOREY - 0.5, 10.5)),
    (ZoneId::Inn, at(-3.5, STOREY - 0.5, 17.5)),
    (ZoneId::Inn, at(3.5, STOREY - 0.5, 10.5)),
    (ZoneId::Inn, at(3.5, STOREY - 0.5, 17.5)),
    (ZoneId::Bakery, at(-19.5, STOREY - 0.5, 10.5)),
    (ZoneId::Bakery, at(-19.5, STOREY - 0.5, 17.5)),
    (ZoneId::Bakery, at(-12.5, STOREY - 0.5, 10.5)),
    (ZoneId::Bakery, at(-12.5, STOREY - 0.5, 17.5)),
    (ZoneId::Smithy, at(-35.5, STOREY - 0.5, 10.5)),
    (ZoneId::Smithy, at(-35.5, STOREY - 0.5, 17.5)),
    (ZoneId::Smithy, at(-28.5, STOREY - 0.5, 10.5)),
    (ZoneId::Smithy, at(-28.5, STOREY - 0.5, 17.5)),
    (ZoneId::Schoolhouse, at(28.5, STOREY - 0.5, 10.5)),
    (ZoneId::Schoolhouse, at(28.5, STOREY - 0.5, 17.5)),
    (ZoneId::Schoolhouse, at(35.5, STOREY - 0.5, 10.5)),
    (ZoneId::Schoolhouse, at(35.5, STOREY - 0.5, 17.5)),
    (ZoneId::Mill, at(-66.0, STOREY - 0.5, 10.5)),
    (ZoneId::Mill, at(-66.0, STOREY - 0.5, 17.5)),
    (ZoneId::Mill, at(-58.0, STOREY - 0.5, 10.5)),
    (ZoneId::Mill, at(-58.0, STOREY - 0.5, 17.5)),
    (ZoneId::Church, at(58.0, STOREY - 0.5, 10.5)),
    (ZoneId::Church, at(58.0, STOREY - 0.5, 17.5)),
    (ZoneId::Church, at(66.0, STOREY - 0.5, 10.5)),
    (ZoneId::Church, at(66.0, STOREY - 0.5, 17.5)),
    (ZoneId::Market, at(-18.4, STOREY - 0.3, -16.7)),
    (ZoneId::Market, at(-18.4, STOREY - 0.3, -8.0)),
    (ZoneId::Market, at(-18.4, STOREY - 0.3, 0.7)),
    (ZoneId::Market, at(-9.2, STOREY - 0.3, -16.7)),
    (ZoneId::Market, at(-9.2, STOREY - 0.3, -8.0)),
    (ZoneId::Market, at(-9.2, STOREY - 0.3, 0.7)),
    (ZoneId::Market, at(0.0, STOREY - 0.3, -16.7)),
    (ZoneId::Market, at(0.0, STOREY - 0.3, -8.0)),
    (ZoneId::Market, at(0.0, STOREY - 0.3, 0.7)),
    (ZoneId::Market, at(9.2, STOREY - 0.3, -16.7)),
    (ZoneId::Market, at(9.2, STOREY - 0.3, -8.0)),
    (ZoneId::Market, at(9.2, STOREY - 0.3, 0.7)),
    (ZoneId::Market, at(18.4, STOREY - 0.3, -16.7)),
    (ZoneId::Market, at(18.4, STOREY - 0.3, -8.0)),
    (ZoneId::Market, at(18.4, STOREY - 0.3, 0.7)),
    (ZoneId::WestEnd, at(-67.7, STOREY - 0.3, 0.0)),
    (ZoneId::WestEnd, at(-61.1, STOREY - 0.3, 0.0)),
    (ZoneId::WestEnd, at(-54.6, STOREY - 0.3, 0.0)),
    (ZoneId::WestEnd, at(-48.0, STOREY - 0.3, 0.0)),
    (ZoneId::WestEnd, at(-41.4, STOREY - 0.3, 0.0)),
    (ZoneId::WestEnd, at(-34.9, STOREY - 0.3, 0.0)),
    (ZoneId::WestEnd, at(-28.3, STOREY - 0.3, 0.0)),
    (ZoneId::EastEnd, at(28.3, STOREY - 0.3, 0.0)),
    (ZoneId::EastEnd, at(34.9, STOREY - 0.3, 0.0)),
    (ZoneId::EastEnd, at(41.4, STOREY - 0.3, 0.0)),
    (ZoneId::EastEnd, at(48.0, STOREY - 0.3, 0.0)),
    (ZoneId::EastEnd, at(54.6, STOREY - 0.3, 0.0)),
    (ZoneId::EastEnd, at(61.1, STOREY - 0.3, 0.0)),
    (ZoneId::EastEnd, at(67.7, STOREY - 0.3, 0.0)),
    (ZoneId::BackLane, at(-18.4, STOREY - 0.3, 26.0)),
    (ZoneId::BackLane, at(-9.2, STOREY - 0.3, 26.0)),
    (ZoneId::BackLane, at(0.0, STOREY - 0.3, 26.0)),
    (ZoneId::BackLane, at(9.2, STOREY - 0.3, 26.0)),
    (ZoneId::BackLane, at(18.4, STOREY - 0.3, 26.0)),
    (ZoneId::MillLane, at(-66.4, STOREY - 0.3, 26.0)),
    (ZoneId::MillLane, at(-57.2, STOREY - 0.3, 26.0)),
    (ZoneId::MillLane, at(-48.0, STOREY - 0.3, 26.0)),
    (ZoneId::MillLane, at(-38.8, STOREY - 0.3, 26.0)),
    (ZoneId::MillLane, at(-29.6, STOREY - 0.3, 26.0)),
    (ZoneId::ChurchLane, at(29.6, STOREY - 0.3, 26.0)),
    (ZoneId::ChurchLane, at(38.8, STOREY - 0.3, 26.0)),
    (ZoneId::ChurchLane, at(48.0, STOREY - 0.3, 26.0)),
    (ZoneId::ChurchLane, at(57.2, STOREY - 0.3, 26.0)),
    (ZoneId::ChurchLane, at(66.4, STOREY - 0.3, 26.0)),
    (ZoneId::Orchard, at(-66.0, STOREY - 0.3, 34.7)),
    (ZoneId::Orchard, at(-66.0, STOREY - 0.3, 42.0)),
    (ZoneId::Orchard, at(-66.0, STOREY - 0.3, 49.3)),
    (ZoneId::Orchard, at(-56.0, STOREY - 0.3, 34.7)),
    (ZoneId::Orchard, at(-56.0, STOREY - 0.3, 42.0)),
    (ZoneId::Orchard, at(-56.0, STOREY - 0.3, 49.3)),
    (ZoneId::Orchard, at(-46.0, STOREY - 0.3, 34.7)),
    (ZoneId::Orchard, at(-46.0, STOREY - 0.3, 42.0)),
    (ZoneId::Orchard, at(-46.0, STOREY - 0.3, 49.3)),
    (ZoneId::Graveyard, at(46.0, STOREY - 0.3, 34.7)),
    (ZoneId::Graveyard, at(46.0, STOREY - 0.3, 42.0)),
    (ZoneId::Graveyard, at(46.0, STOREY - 0.3, 49.3)),
    (ZoneId::Graveyard, at(56.0, STOREY - 0.3, 34.7)),
    (ZoneId::Graveyard, at(56.0, STOREY - 0.3, 42.0)),
    (ZoneId::Graveyard, at(56.0, STOREY - 0.3, 49.3)),
    (ZoneId::Graveyard, at(66.0, STOREY - 0.3, 34.7)),
    (ZoneId::Graveyard, at(66.0, STOREY - 0.3, 42.0)),
    (ZoneId::Graveyard, at(66.0, STOREY - 0.3, 49.3)),
];

/// Old dark stone, for the chancel and its steps.
const CHANCEL_STONE: Color = Color::srgb(0.38, 0.35, 0.33);

/// The Church's chancel: a dais across its east end, Pack-a-Punch at its south end and the altar
/// against the east wall, up a flight of steps from the aisle.
const CHANCEL: RaisedFloor = RaisedFloor { zone: ZoneId::Church, min: v(66.0, 8.0), max: v(71.8, 20.0), height: 0.45, colour: CHANCEL_STONE };

/// The Infills, one to three in each building, so no two share a shape. Each keeps clear of the
/// Doors, windows, Buyables and Spawn Points, and of every path-node edge.
const INFILLS: [Infill; 14] = {
    use ZoneId::*;
    [
        // The Inn: a chimney breast in the north-west corner, behind the bar, and a cupboard in the
        // south-east corner.
        infill(Inn, (-8.0, 21.0), (-6.6, 22.0)),
        infill(Inn, (6.4, 6.0), (8.0, 7.4)),
        // The Bakery: a chimney in the south-west corner and a flue in the north-west.
        infill(Bakery, (-24.0, 6.0), (-22.2, 7.6)),
        infill(Bakery, (-24.0, 20.8), (-22.8, 22.0)),
        // The Smithy: the forge chimney in the south-west corner and a quench-room nook in the south-east.
        infill(Smithy, (-40.0, 6.0), (-38.6, 7.4)),
        infill(Smithy, (-25.8, 6.0), (-24.0, 7.2)),
        // The Schoolhouse: a stove in the north-west corner, a pier on the north wall and a cupboard in the south-east.
        infill(Schoolhouse, (24.0, 21.0), (25.2, 22.0)),
        infill(Schoolhouse, (27.5, 21.0), (28.5, 22.0)),
        infill(Schoolhouse, (38.6, 6.0), (40.0, 7.4)),
        // The Mill: a grain bin in the south-west, the loft stair on the north wall, and the wheel chute in the south-east.
        infill(Mill, (-72.0, 6.0), (-70.0, 8.0)),
        infill(Mill, (-68.5, 20.6), (-66.5, 22.0)),
        infill(Mill, (-53.6, 6.0), (-52.0, 7.6)),
        // The Church: a vestry in the north-west corner and a buttress in the south-west.
        infill(Church, (52.0, 20.0), (54.2, 22.0)),
        infill(Church, (52.0, 6.0), (53.6, 7.8)),
    ]
};

const SLOPES: [Slope; 1] = [Slope {
    zone: ZoneId::Church,
    foot: at(66.0 - 1.2, 0.0, 14.0),
    up: Vec2::X,
    run: 1.2,
    rise: CHANCEL.height,
    width: 3.0,
    steps: true,
    colour: CHANCEL_STONE,
}];

/// The Props. Most stand flush against a wall, so nothing hides behind them; the islands to run
/// round are the Market's cross and the Graveyard's mausoleum (both built with the walls) and the
/// Mill's millstones.
const PROPS: &[Prop] = {
    use PropKind::*;
    use ZoneId::*;
    &[
        // The Inn: the bar along its wall to the Bakery, and two tables.
        prop(Counter, Inn, (-7.4, 0.0, 14.0), (0.8, 1.1, 5.0)),
        prop(Table, Inn, (-3.5, 0.0, 9.5), (1.4, 0.76, 1.4)),
        prop(Table, Inn, (-3.5, 0.0, 18.5), (1.4, 0.76, 1.4)),
        // The Market: a stall either side of the market cross (built with the walls).
        prop(Counter, Market, (-14.0, 0.0, -16.0), (2.6, 1.0, 1.0)),
        prop(Counter, Market, (14.0, 0.0, -16.0), (2.6, 1.0, 1.0)),
        // The West End and the East End: a cart against the south side, and drums against the
        // garden walls.
        prop(Machine, WestEnd, (-42.0, 0.0, -5.1), (3.0, 1.4, 1.4)),
        prop(Barrel, WestEnd, (-46.0, 0.0, 5.4), (0.65, 0.95, 0.65)),
        prop(Barrel, WestEnd, (-45.2, 0.0, 5.45), (0.65, 0.95, 0.65)),
        prop(Machine, EastEnd, (42.0, 0.0, -5.1), (3.0, 1.4, 1.4)),
        prop(Barrel, EastEnd, (46.0, 0.0, 5.4), (0.65, 0.95, 0.65)),
        // The lanes: drums and a crate against their north walls.
        prop(Barrel, BackLane, (20.5, 0.0, 29.4), (0.65, 0.95, 0.65)),
        prop(Barrel, BackLane, (21.3, 0.0, 29.45), (0.65, 0.95, 0.65)),
        prop(Crate, MillLane, (-28.5, 0.0, 29.3), (1.0, 0.8, 1.0)),
        prop(Barrel, ChurchLane, (28.6, 0.0, 29.4), (0.65, 0.95, 0.65)),
        // The Mill: the millstones in the middle, and sacks in a corner.
        prop(Machine, Mill, (-62.0, 0.0, 14.0), (3.0, 1.3, 3.0)),
        prop(Crate, Mill, (-52.7, 0.0, 21.3), (1.0, 0.8, 1.0)),
        // The Smithy: the forge against its east wall, and the anvil before it.
        prop(Machine, Smithy, (-25.2, 0.0, 14.0), (2.0, 1.6, 2.4)),
        prop(Machine, Smithy, (-29.0, 0.0, 14.0), (0.6, 0.8, 1.0)),
        // The Bakery: the oven against its west wall, the kneading table, and flour sacks.
        prop(Machine, Bakery, (-22.8, 0.0, 14.0), (2.0, 2.0, 3.0)),
        prop(Table, Bakery, (-14.5, 0.0, 14.0), (2.4, 0.9, 1.0)),
        prop(Sandbags, Bakery, (-10.0, 0.0, 21.4), (2.0, 0.6, 0.8)),
        // The Schoolhouse: two rows of desks either side of the aisle.
        prop(Table, Schoolhouse, (28.5, 0.0, 10.5), (3.0, 0.76, 0.8)),
        prop(Table, Schoolhouse, (35.5, 0.0, 10.5), (3.0, 0.76, 0.8)),
        prop(Table, Schoolhouse, (28.5, 0.0, 17.5), (3.0, 0.76, 0.8)),
        prop(Table, Schoolhouse, (35.5, 0.0, 17.5), (3.0, 0.76, 0.8)),
        // The Church: four pews either side of the aisle, and the altar up on the chancel.
        prop(Bench, Church, (60.5, 0.0, 10.5), (0.6, 0.5, 3.0)),
        prop(Bench, Church, (63.0, 0.0, 10.5), (0.6, 0.5, 3.0)),
        prop(Bench, Church, (60.5, 0.0, 17.5), (0.6, 0.5, 3.0)),
        prop(Bench, Church, (63.0, 0.0, 17.5), (0.6, 0.5, 3.0)),
        prop(Altar, Church, (71.3, CHANCEL.height, 15.5), (0.9, 1.0, 2.4)),
        // The Orchard: two staggered rows of trees.
        prop(Tree, Orchard, (-64.0, 0.0, 37.5), (0.5, 3.2, 0.5)),
        prop(Tree, Orchard, (-56.0, 0.0, 37.5), (0.5, 3.2, 0.5)),
        prop(Tree, Orchard, (-48.0, 0.0, 37.5), (0.5, 3.2, 0.5)),
        prop(Tree, Orchard, (-68.0, 0.0, 46.5), (0.5, 3.2, 0.5)),
        prop(Tree, Orchard, (-60.0, 0.0, 46.5), (0.5, 3.2, 0.5)),
        prop(Tree, Orchard, (-52.0, 0.0, 46.5), (0.5, 3.2, 0.5)),
        prop(Tree, Orchard, (-44.0, 0.0, 46.5), (0.5, 3.2, 0.5)),
        // The Graveyard: rows of graves either side of the mausoleum.
        prop(Headstone, Graveyard, (44.0, 0.0, 35.0), HEADSTONE),
        prop(Headstone, Graveyard, (47.0, 0.0, 35.0), HEADSTONE),
        prop(Headstone, Graveyard, (44.0, 0.0, 39.0), HEADSTONE),
        prop(Headstone, Graveyard, (47.0, 0.0, 39.0), HEADSTONE),
        prop(Headstone, Graveyard, (44.0, 0.0, 43.0), HEADSTONE),
        prop(Headstone, Graveyard, (47.0, 0.0, 43.0), HEADSTONE),
        prop(Headstone, Graveyard, (44.0, 0.0, 47.0), HEADSTONE),
        prop(Headstone, Graveyard, (47.0, 0.0, 47.0), HEADSTONE),
        prop(Headstone, Graveyard, (44.0, 0.0, 51.0), HEADSTONE),
        prop(Headstone, Graveyard, (47.0, 0.0, 51.0), HEADSTONE),
        prop(Headstone, Graveyard, (65.0, 0.0, 35.0), HEADSTONE),
        prop(Headstone, Graveyard, (68.0, 0.0, 35.0), HEADSTONE),
        prop(Headstone, Graveyard, (65.0, 0.0, 39.0), HEADSTONE),
        prop(Headstone, Graveyard, (68.0, 0.0, 39.0), HEADSTONE),
        prop(Headstone, Graveyard, (65.0, 0.0, 43.0), HEADSTONE),
        prop(Headstone, Graveyard, (68.0, 0.0, 43.0), HEADSTONE),
        prop(Headstone, Graveyard, (65.0, 0.0, 47.0), HEADSTONE),
        prop(Headstone, Graveyard, (68.0, 0.0, 47.0), HEADSTONE),
        prop(Headstone, Graveyard, (65.0, 0.0, 51.0), HEADSTONE),
        prop(Headstone, Graveyard, (68.0, 0.0, 51.0), HEADSTONE),
    ]
};

/// The middle of the market cross (x, z).
const MARKET_CROSS: Vec2 = v(0.0, -8.0);
/// Weathered limestone, for the market cross.
const CROSS_STONE: Color = Color::srgb(0.55, 0.53, 0.47);
/// How wide the market cross's base is each way, and how tall: wide enough to run a Train round,
/// and too tall for a Player to clamber onto.
const MARKET_CROSS_BASE: f32 = 5.0;
const MARKET_CROSS_BASE_HEIGHT: f32 = 1.5;

/// A headstone's size: wide across the row, waist-high and thin.
const HEADSTONE: (f32, f32, f32) = (0.8, 0.9, 0.25);

/// The mausoleum in the middle of the Graveyard: its footprint's corners (x, z) and how tall it is.
const MAUSOLEUM: (Vec2, Vec2, f32) = (v(53.0, 40.5), v(59.0, 45.5), 3.5);

/// The walls, the Doors, the gates, the ground round the village and its landmarks.
fn build(b: &mut Builder) {
    use ZoneId::*;

    // Ground outside the walls, where Zombies gather at the windows: the fields to the south,
    // the verges at either end, the gardens between the buildings and the river bank to the
    // north.
    for (min, max) in [
        (v(-80.0, -30.0), v(-24.0, -6.0)),
        (v(-24.0, -30.0), v(24.0, -22.0)),
        (v(24.0, -30.0), v(80.0, -6.0)),
        (v(-80.0, -6.0), v(-72.0, 60.0)),
        (v(72.0, -6.0), v(80.0, 60.0)),
        (v(-52.0, 6.0), v(-40.0, 22.0)),
        (v(8.0, 6.0), v(24.0, 22.0)),
        (v(40.0, 6.0), v(52.0, 22.0)),
        (v(-72.0, 54.0), v(-40.0, 60.0)),
        (v(-40.0, 30.0), v(40.0, 60.0)),
        (v(40.0, 54.0), v(72.0, 60.0)),
    ] {
        let (size, mid) = (max - min, (min + max) / 2.0);
        b.ground(Vec3::new(size.x, 0.2, size.y), Vec3::new(mid.x, -0.1, mid.y));
    }

    // The walls round the village: down each end, along the south of the High Street and the
    // Market, and round the Orchard and the Graveyard.
    b.outer_wall(v(-72.0, -6.0), v(-72.0, 54.0), 0.0, STOREY);
    b.outer_wall(v(72.0, -6.0), v(72.0, 54.0), 0.0, STOREY);
    b.outer_wall(v(-72.0, -6.0), v(-24.0, -6.0), 0.0, STOREY);
    b.outer_wall(v(24.0, -6.0), v(72.0, -6.0), 0.0, STOREY);
    b.outer_wall(v(-24.0, -22.0), v(24.0, -22.0), 0.0, STOREY);
    b.outer_wall(v(-24.0, -22.0), v(-24.0, -6.0), 0.0, STOREY);
    b.outer_wall(v(24.0, -22.0), v(24.0, -6.0), 0.0, STOREY);
    b.outer_wall(v(-72.0, 54.0), v(-40.0, 54.0), 0.0, STOREY);
    b.outer_wall(v(-40.0, 30.0), v(-40.0, 54.0), 0.0, STOREY);
    b.outer_wall(v(40.0, 54.0), v(72.0, 54.0), 0.0, STOREY);
    b.outer_wall(v(40.0, 30.0), v(40.0, 54.0), 0.0, STOREY);

    // The barricades across the High Street either side of the Market.
    let barricade = DOOR_HEIGHT + 0.4;
    b.wall(v(-24.0, -6.0), v(-24.0, 6.0), 0.0, barricade, &[(6.0, door(Market, WestEnd, DOOR_PRICE_MARKET_WEST_END))]);
    b.wall(v(24.0, -6.0), v(24.0, 6.0), 0.0, barricade, &[(6.0, door(Market, EastEnd, DOOR_PRICE_MARKET_EAST_END))]);

    // The fronts along the High Street, with a Door into every building and a window from the
    // Market into the Inn's garden; and the backs along the lanes, a Door out of every building
    // and a window from the Back Lane into the same garden.
    b.wall(
        v(-72.0, 6.0),
        v(72.0, 6.0),
        0.0,
        STOREY,
        &[
            (10.0, door(WestEnd, Mill, DOOR_PRICE_WEST_END_MILL)),
            (40.0, door(WestEnd, Smithy, DOOR_PRICE_WEST_END_SMITHY)),
            (56.0, door(Market, Bakery, DOOR_PRICE_MARKET_BAKERY)),
            (72.0, door(Inn, Market, DOOR_PRICE_INN_MARKET)),
            (88.0, Gap::Window),
            (104.0, door(EastEnd, Schoolhouse, DOOR_PRICE_EAST_END_SCHOOLHOUSE)),
            (130.0, door(EastEnd, Church, DOOR_PRICE_EAST_END_CHURCH)),
        ],
    );
    b.wall(
        v(-72.0, 22.0),
        v(72.0, 22.0),
        0.0,
        STOREY,
        &[
            (10.0, door(Mill, MillLane, DOOR_PRICE_MILL_MILL_LANE)),
            (40.0, door(Smithy, MillLane, DOOR_PRICE_SMITHY_MILL_LANE)),
            (56.0, door(Bakery, BackLane, DOOR_PRICE_BAKERY_BACK_LANE)),
            (72.0, door(Inn, BackLane, DOOR_PRICE_INN_BACK_LANE)),
            (88.0, Gap::Window),
            (104.0, door(Schoolhouse, ChurchLane, DOOR_PRICE_SCHOOLHOUSE_CHURCH_LANE)),
            (130.0, door(Church, ChurchLane, DOOR_PRICE_CHURCH_CHURCH_LANE)),
        ],
    );

    // The buildings' side walls: onto a garden, with windows, or shared with the next building.
    for x in [-52.0, -40.0, 8.0, 24.0, 40.0, 52.0] {
        b.outer_wall(v(x, 6.0), v(x, 22.0), 0.0, STOREY);
    }
    for x in [-24.0, -8.0] {
        b.wall(v(x, 6.0), v(x, 22.0), 0.0, STOREY, &[]);
    }
    // Their upper storeys, over the streets and the lanes.
    for (x0, x1) in [(-72.0, -52.0), (-40.0, 8.0), (24.0, 40.0), (52.0, 72.0)] {
        for z in [6.0, 22.0] {
            b.wall(v(x0, z), v(x1, z), STOREY, 0.75 * STOREY, &[]);
        }
        for x in [x0, x1] {
            b.wall(v(x, 6.0), v(x, 22.0), STOREY, 0.75 * STOREY, &[]);
        }
    }

    // The gates in the lanes either side of the Back Lane, opened by the Power.
    b.wall(v(-24.0, 22.0), v(-24.0, 30.0), 0.0, STOREY, &[(4.0, Gap::Door(Some((BackLane, MillLane)), None))]);
    b.wall(v(24.0, 22.0), v(24.0, 30.0), 0.0, STOREY, &[(4.0, Gap::Door(Some((BackLane, ChurchLane)), None))]);

    // The lanes' north wall, onto the river bank, with the gateways into the Orchard and the
    // Graveyard.
    let gateway = || Gap::Open { width: 4.0, height: DOOR_HEIGHT };
    b.wall(
        v(-72.0, 30.0),
        v(72.0, 30.0),
        0.0,
        STOREY,
        &[(16.0, gateway()), (40.0, Gap::Window), (60.0, Gap::Window), (84.0, Gap::Window), (104.0, Gap::Window), (128.0, gateway())],
    );

    // The mausoleum, an island in the middle of the Graveyard, under a heavier cap.
    let stone = palette(Some(Graveyard)).wall;
    let (min, max, tall) = MAUSOLEUM;
    let (size, mid) = (max - min, (min + max) / 2.0);
    b.block_in(Vec3::new(size.x, tall, size.y), Vec3::new(mid.x, tall / 2.0, mid.y), stone, super::materials(Some(Graveyard)).map(|set| set.wall));
    b.block(Vec3::new(size.x + 0.4, 0.4, size.y + 0.4), Vec3::new(mid.x, tall + 0.2, mid.y), palette(Some(Graveyard)).trim);

    // The market cross in the middle of the Market: its shaft on a broad base of sheer stone, with
    // a moulding round the base's top and a cap on the shaft.
    let trim = palette(Some(Market)).trim;
    let (base, high) = (MARKET_CROSS_BASE, MARKET_CROSS_BASE_HEIGHT);
    let cross = |y: f32| Vec3::new(MARKET_CROSS.x, y, MARKET_CROSS.y);
    b.block(Vec3::new(base, high, base), cross(high / 2.0), CROSS_STONE);
    b.block(Vec3::new(base + 0.2, 0.25, base + 0.2), cross(high - 0.125), trim);
    b.block(Vec3::new(0.9, 3.0, 0.9), cross(high + 1.5), CROSS_STONE);
    b.block(Vec3::new(1.3, 0.3, 1.3), cross(high + 3.15), trim);

    // The church tower, in the garden by the Church, standing over the whole village.
    let tower = palette(Some(Church)).wall;
    b.block_in(Vec3::new(4.0, 14.0, 4.0), at(46.0, 7.0, 19.5), tower, super::materials(Some(Church)).map(|set| set.wall));
    b.block(Vec3::new(4.6, 0.6, 4.6), at(46.0, 14.3, 19.5), palette(Some(Church)).trim);
    // The Mill's wheel, on its west end, out over the verge.
    b.block(Vec3::new(0.6, 6.0, 6.0), at(-74.5, 2.0, 14.0), palette(Some(Mill)).trim);
}

/// The path nodes: a name, then where a Zombie's feet stand there (x, y, z).
const NODES: &[(&str, f32, f32, f32)] = &[
    // The Inn, round its tables and along its bar
    ("inn", 0.0, 0.0, 14.0),
    ("inn_s", 0.0, 0.0, 7.5),
    ("inn_n", 0.0, 0.0, 20.5),
    ("inn_w", -5.5, 0.0, 14.0),
    ("inn_sw", -6.0, 0.0, 7.5),
    ("inn_nw", -6.0, 0.0, 20.5),
    ("inn_e", 5.5, 0.0, 14.0),
    ("w0_in", 6.5, 0.0, 10.0),
    ("w0_out", 10.0, 0.0, 10.0),
    ("w1_in", 6.5, 0.0, 18.0),
    ("w1_out", 10.0, 0.0, 18.0),
    // The Market, round the cross
    ("mk_inn", 0.0, 0.0, 4.0),
    ("mk_bakery", -16.0, 0.0, 4.0),
    ("mk_wdoor", -22.5, 0.0, 0.0),
    ("mk_edoor", 22.5, 0.0, 0.0),
    ("mk_nw", -16.0, 0.0, -3.0),
    ("mk_ne", 16.0, 0.0, -3.0),
    ("mk_cn", 0.0, 0.0, -1.5),
    ("mk_cw", -6.5, 0.0, -8.0),
    ("mk_ce", 6.5, 0.0, -8.0),
    ("mk_cs", 0.0, 0.0, -14.5),
    ("mk_w", -20.0, 0.0, -11.0),
    ("mk_e", 20.0, 0.0, -11.0),
    ("mk_sw", -19.0, 0.0, -19.0),
    ("mk_s", 0.0, 0.0, -19.0),
    ("mk_se", 19.0, 0.0, -19.0),
    ("w2_in", -12.0, 0.0, -20.5),
    ("w2_out", -12.0, 0.0, -24.0),
    ("w3_in", 12.0, 0.0, -20.5),
    ("w3_out", 12.0, 0.0, -24.0),
    ("w4_in", -22.5, 0.0, -14.0),
    ("w4_out", -26.0, 0.0, -14.0),
    ("w5_in", 22.5, 0.0, -14.0),
    ("w5_out", 26.0, 0.0, -14.0),
    ("w6_in", 16.0, 0.0, 4.5),
    ("w6_out", 16.0, 0.0, 8.0),
    // The West End
    ("we_door", -25.5, 0.0, 0.0),
    ("we_1", -32.0, 0.0, 0.0),
    ("we_smithy", -32.0, 0.0, 4.5),
    ("we_2", -42.0, 0.0, 0.0),
    ("we_3", -52.0, 0.0, 0.0),
    ("we_4", -62.0, 0.0, 0.0),
    ("we_mill", -62.0, 0.0, 4.5),
    ("we_w", -68.0, 0.0, 0.0),
    ("w7_in", -60.0, 0.0, -4.5),
    ("w7_out", -60.0, 0.0, -8.0),
    ("w8_in", -36.0, 0.0, -4.5),
    ("w8_out", -36.0, 0.0, -8.0),
    ("w9_in", -70.5, 0.0, 0.0),
    ("w9_out", -74.0, 0.0, 0.0),
    // The East End
    ("ee_door", 25.5, 0.0, 0.0),
    ("ee_1", 32.0, 0.0, 0.0),
    ("ee_school", 32.0, 0.0, 4.5),
    ("ee_2", 42.0, 0.0, 0.0),
    ("ee_3", 50.0, 0.0, 0.0),
    ("ee_4", 58.0, 0.0, 0.0),
    ("ee_church", 58.0, 0.0, 4.5),
    ("ee_e", 68.0, 0.0, 0.0),
    ("w10_in", 36.0, 0.0, -4.5),
    ("w10_out", 36.0, 0.0, -8.0),
    ("w11_in", 60.0, 0.0, -4.5),
    ("w11_out", 60.0, 0.0, -8.0),
    ("w12_in", 70.5, 0.0, 0.0),
    ("w12_out", 74.0, 0.0, 0.0),
    // The Mill, round the millstones
    ("mill_s", -62.0, 0.0, 7.5),
    ("mill_n", -62.0, 0.0, 20.5),
    ("mill_sw", -68.0, 0.0, 9.0),
    ("mill_nw", -68.0, 0.0, 19.0),
    ("mill_se", -56.0, 0.0, 9.0),
    ("mill_ne", -56.0, 0.0, 19.0),
    ("w13_in", -70.5, 0.0, 14.0),
    ("w13_out", -74.0, 0.0, 14.0),
    ("w14_in", -53.5, 0.0, 14.0),
    ("w14_out", -50.0, 0.0, 14.0),
    // The Smithy, round the anvil
    ("sm_s", -32.0, 0.0, 7.5),
    ("sm_n", -32.0, 0.0, 20.5),
    ("sm_c", -32.0, 0.0, 14.0),
    ("sm_sw", -38.0, 0.0, 8.5),
    ("sm_nw", -38.0, 0.0, 19.0),
    ("sm_se", -27.0, 0.0, 8.5),
    ("sm_ne", -27.0, 0.0, 19.5),
    ("w15_in", -38.5, 0.0, 14.0),
    ("w15_out", -42.0, 0.0, 14.0),
    // The Bakery, round the kneading table
    ("bk_s", -16.0, 0.0, 7.5),
    ("bk_n", -16.0, 0.0, 20.5),
    ("bk_c", -18.0, 0.0, 14.0),
    ("bk_sw", -20.5, 0.0, 8.0),
    ("bk_nw", -20.5, 0.0, 20.0),
    ("bk_se", -10.5, 0.0, 8.0),
    ("bk_e", -11.0, 0.0, 14.0),
    ("bk_ne", -11.0, 0.0, 19.5),
    // The Schoolhouse, down its aisle and round the desks
    ("sc_s", 32.0, 0.0, 7.5),
    ("sc_n", 32.0, 0.0, 20.5),
    ("sc_c", 32.0, 0.0, 14.0),
    ("sc_sw", 25.6, 0.0, 8.2),
    ("sc_nw", 25.6, 0.0, 19.8),
    ("sc_se", 38.4, 0.0, 8.2),
    ("sc_ne", 38.4, 0.0, 19.8),
    ("w16_in", 25.5, 0.0, 14.0),
    ("w16_out", 22.0, 0.0, 14.0),
    ("w17_in", 38.5, 0.0, 14.0),
    ("w17_out", 42.0, 0.0, 14.0),
    // The Church: the nave, the aisle to the chancel steps, and up on the chancel
    ("ch_s", 58.0, 0.0, 7.5),
    ("ch_n", 58.0, 0.0, 20.5),
    ("ch_c", 58.0, 0.0, 14.0),
    ("ch_pew_s", 61.75, 0.0, 7.6),
    ("ch_pew_n", 61.75, 0.0, 20.4),
    ("ch_se", 65.0, 0.0, 7.2),
    ("ch_ne", 65.0, 0.0, 20.8),
    ("ch_e_s", 70.8, 0.0, 7.1),
    ("ch_e_n", 70.8, 0.0, 20.9),
    ("ch_aisle", 64.2, 0.0, 14.0),
    ("ch_dais", 67.8, CHANCEL.height, 14.0),
    ("w18_in", 53.5, 0.0, 14.0),
    ("w18_out", 50.0, 0.0, 14.0),
    // The Back Lane
    ("bl_w", -22.5, 0.0, 26.0),
    ("bl_a", -16.0, 0.0, 26.0),
    ("bl_bakery", -16.0, 0.0, 23.5),
    ("bl_b", -8.0, 0.0, 26.0),
    ("bl_c", 0.0, 0.0, 26.0),
    ("bl_inn", 0.0, 0.0, 23.5),
    ("bl_d", 8.0, 0.0, 26.0),
    ("bl_f", 16.0, 0.0, 26.0),
    ("bl_e", 22.5, 0.0, 26.0),
    ("w19_in", -12.0, 0.0, 28.5),
    ("w19_out", -12.0, 0.0, 32.0),
    ("w20_in", 12.0, 0.0, 28.5),
    ("w20_out", 12.0, 0.0, 32.0),
    ("w21_in", 16.0, 0.0, 23.5),
    ("w21_out", 16.0, 0.0, 20.0),
    // Mill Lane
    ("ml_e", -25.5, 0.0, 26.0),
    ("ml_a", -32.0, 0.0, 26.0),
    ("ml_smithy", -32.0, 0.0, 23.5),
    ("ml_b", -44.0, 0.0, 26.0),
    ("ml_c", -56.0, 0.0, 26.0),
    ("ml_d", -62.0, 0.0, 26.0),
    ("ml_mill", -62.0, 0.0, 23.5),
    ("ml_w", -68.0, 0.0, 26.0),
    ("w22_in", -32.0, 0.0, 28.5),
    ("w22_out", -32.0, 0.0, 32.0),
    ("w23_in", -70.5, 0.0, 26.0),
    ("w23_out", -74.0, 0.0, 26.0),
    // Church Lane
    ("cl_w", 25.5, 0.0, 26.0),
    ("cl_a", 32.0, 0.0, 26.0),
    ("cl_school", 32.0, 0.0, 23.5),
    ("cl_b", 44.0, 0.0, 26.0),
    ("cl_c", 56.0, 0.0, 26.0),
    ("cl_church", 58.0, 0.0, 23.5),
    ("cl_e", 68.0, 0.0, 26.0),
    ("w24_in", 32.0, 0.0, 28.5),
    ("w24_out", 32.0, 0.0, 32.0),
    ("w25_in", 70.5, 0.0, 26.0),
    ("w25_out", 74.0, 0.0, 26.0),
    // The Orchard, in a grid between the rows of trees
    ("or_gate", -56.0, 0.0, 32.0),
    ("or_a1", -66.0, 0.0, 33.0),
    ("or_a2", -58.0, 0.0, 33.0),
    ("or_a3", -50.0, 0.0, 33.0),
    ("or_a4", -42.0, 0.0, 33.0),
    ("or_b1", -66.0, 0.0, 42.0),
    ("or_b2", -58.0, 0.0, 42.0),
    ("or_b3", -50.0, 0.0, 42.0),
    ("or_b4", -42.0, 0.0, 42.0),
    ("or_c1", -66.0, 0.0, 51.0),
    ("or_c2", -58.0, 0.0, 51.0),
    ("or_c3", -50.0, 0.0, 51.0),
    ("or_c4", -42.0, 0.0, 51.0),
    ("w26_in", -70.5, 0.0, 42.0),
    ("w26_out", -74.0, 0.0, 42.0),
    ("w27_in", -56.0, 0.0, 52.5),
    ("w27_out", -56.0, 0.0, 56.0),
    ("w28_in", -41.5, 0.0, 44.0),
    ("w28_out", -38.0, 0.0, 44.0),
    // The Graveyard: round the mausoleum, and down the paths outside the rows of graves
    ("gy_gate", 56.0, 0.0, 32.0),
    ("gy_s", 56.0, 0.0, 36.3),
    ("gy_sw", 51.0, 0.0, 37.8),
    ("gy_se", 61.0, 0.0, 37.8),
    ("gy_nw", 51.0, 0.0, 47.8),
    ("gy_ne", 61.0, 0.0, 47.8),
    ("gw_1", 42.0, 0.0, 33.0),
    ("gw_2", 42.0, 0.0, 37.0),
    ("gw_3", 42.0, 0.0, 41.0),
    ("gw_5", 42.0, 0.0, 49.0),
    ("gw_6", 42.0, 0.0, 52.8),
    ("ge_1", 70.0, 0.0, 33.0),
    ("ge_2", 70.0, 0.0, 37.0),
    ("ge_4", 70.0, 0.0, 45.0),
    ("ge_5", 70.0, 0.0, 49.0),
    ("ge_6", 70.0, 0.0, 52.8),
    ("w29_in", 41.5, 0.0, 44.0),
    ("w29_out", 38.0, 0.0, 44.0),
    ("w30_in", 56.0, 0.0, 52.5),
    ("w30_out", 56.0, 0.0, 56.0),
    ("w31_in", 70.5, 0.0, 42.0),
    ("w31_out", 74.0, 0.0, 42.0),
    // Vents, up near the rafters, over the spot a Nova Crawler drops to
    ("vent_bakery", -18.0, NOVA_VENT_HEIGHT, 14.0),
    ("vent_smithy", -32.0, NOVA_VENT_HEIGHT, 14.0),
    ("vent_school", 32.0, NOVA_VENT_HEIGHT, 14.0),
    ("vent_church", 58.0, NOVA_VENT_HEIGHT, 14.0),
];

const OPEN: Gate = Gate::Open;

/// The edges, by node name. Barrier and one-way edges run from the first node to the second.
const EDGES: &[(&str, &str, Gate)] = {
    use ZoneId::*;
    &[
        // The Inn
        ("inn", "inn_s", OPEN),
        ("inn", "inn_n", OPEN),
        ("inn", "inn_w", OPEN),
        ("inn", "inn_e", OPEN),
        ("inn_w", "inn_sw", OPEN),
        ("inn_w", "inn_nw", OPEN),
        ("inn_s", "inn_sw", OPEN),
        ("inn_n", "inn_nw", OPEN),
        ("inn_e", "w0_in", OPEN),
        ("inn_e", "w1_in", OPEN),
        ("inn_s", "w0_in", OPEN),
        ("inn_n", "w1_in", OPEN),
        ("w0_out", "w0_in", barrier(0)),
        ("w1_out", "w1_in", barrier(1)),
        ("inn_s", "mk_inn", Gate::Door(Inn, Market)),
        ("inn_n", "bl_inn", Gate::Door(Inn, BackLane)),
        // The Market
        ("mk_inn", "mk_cn", OPEN),
        ("mk_inn", "mk_nw", OPEN),
        ("mk_inn", "mk_ne", OPEN),
        ("mk_bakery", "mk_nw", OPEN),
        ("mk_bakery", "mk_wdoor", OPEN),
        ("w6_in", "mk_ne", OPEN),
        ("w6_in", "mk_edoor", OPEN),
        ("mk_nw", "mk_wdoor", OPEN),
        ("mk_nw", "mk_cn", OPEN),
        ("mk_nw", "mk_cw", OPEN),
        ("mk_nw", "mk_w", OPEN),
        ("mk_ne", "mk_edoor", OPEN),
        ("mk_ne", "mk_cn", OPEN),
        ("mk_ne", "mk_ce", OPEN),
        ("mk_ne", "mk_e", OPEN),
        ("mk_cn", "mk_cw", OPEN),
        ("mk_cw", "mk_cs", OPEN),
        ("mk_cs", "mk_ce", OPEN),
        ("mk_ce", "mk_cn", OPEN),
        ("mk_w", "mk_cw", OPEN),
        ("mk_e", "mk_ce", OPEN),
        ("mk_w", "w4_in", OPEN),
        ("mk_e", "w5_in", OPEN),
        ("mk_w", "mk_sw", OPEN),
        ("mk_e", "mk_se", OPEN),
        ("mk_cs", "mk_s", OPEN),
        ("mk_s", "mk_sw", OPEN),
        ("mk_s", "mk_se", OPEN),
        ("mk_s", "w2_in", OPEN),
        ("mk_s", "w3_in", OPEN),
        ("mk_sw", "w2_in", OPEN),
        ("mk_se", "w3_in", OPEN),
        ("w2_out", "w2_in", barrier(2)),
        ("w3_out", "w3_in", barrier(3)),
        ("w4_out", "w4_in", barrier(4)),
        ("w5_out", "w5_in", barrier(5)),
        ("w6_out", "w6_in", barrier(6)),
        ("mk_bakery", "bk_s", Gate::Door(Market, Bakery)),
        ("mk_wdoor", "we_door", Gate::Door(Market, WestEnd)),
        ("mk_edoor", "ee_door", Gate::Door(Market, EastEnd)),
        // The West End
        ("we_door", "we_1", OPEN),
        ("we_1", "we_smithy", OPEN),
        ("we_1", "w8_in", OPEN),
        ("we_1", "we_2", OPEN),
        ("we_2", "w8_in", OPEN),
        ("we_2", "we_3", OPEN),
        ("we_3", "we_4", OPEN),
        ("we_3", "w7_in", OPEN),
        ("we_4", "w7_in", OPEN),
        ("we_4", "we_mill", OPEN),
        ("we_4", "we_w", OPEN),
        ("we_w", "w9_in", OPEN),
        ("w7_out", "w7_in", barrier(7)),
        ("w8_out", "w8_in", barrier(8)),
        ("w9_out", "w9_in", barrier(9)),
        ("we_smithy", "sm_s", Gate::Door(WestEnd, Smithy)),
        ("we_mill", "mill_s", Gate::Door(WestEnd, Mill)),
        // The East End
        ("ee_door", "ee_1", OPEN),
        ("ee_1", "ee_school", OPEN),
        ("ee_1", "w10_in", OPEN),
        ("ee_1", "ee_2", OPEN),
        ("ee_2", "w10_in", OPEN),
        ("ee_2", "ee_3", OPEN),
        ("ee_3", "ee_4", OPEN),
        ("ee_3", "w11_in", OPEN),
        ("ee_4", "w11_in", OPEN),
        ("ee_4", "ee_church", OPEN),
        ("ee_4", "ee_e", OPEN),
        ("ee_e", "w11_in", OPEN),
        ("ee_e", "w12_in", OPEN),
        ("w10_out", "w10_in", barrier(10)),
        ("w11_out", "w11_in", barrier(11)),
        ("w12_out", "w12_in", barrier(12)),
        ("ee_school", "sc_s", Gate::Door(EastEnd, Schoolhouse)),
        ("ee_church", "ch_s", Gate::Door(EastEnd, Church)),
        // The Mill
        ("mill_s", "mill_sw", OPEN),
        ("mill_s", "mill_se", OPEN),
        ("mill_n", "mill_nw", OPEN),
        ("mill_n", "mill_ne", OPEN),
        ("mill_sw", "mill_nw", OPEN),
        ("mill_se", "mill_ne", OPEN),
        ("mill_sw", "w13_in", OPEN),
        ("mill_nw", "w13_in", OPEN),
        ("mill_se", "w14_in", OPEN),
        ("mill_ne", "w14_in", OPEN),
        ("w13_out", "w13_in", barrier(13)),
        ("w14_out", "w14_in", barrier(14)),
        ("mill_n", "ml_mill", Gate::Door(Mill, MillLane)),
        // The Smithy
        ("sm_s", "sm_c", OPEN),
        ("sm_c", "sm_n", OPEN),
        ("sm_c", "w15_in", OPEN),
        ("sm_s", "sm_sw", OPEN),
        ("sm_sw", "w15_in", OPEN),
        ("sm_n", "sm_nw", OPEN),
        ("sm_nw", "w15_in", OPEN),
        ("sm_s", "sm_se", OPEN),
        ("sm_n", "sm_ne", OPEN),
        ("sm_se", "sm_ne", OPEN),
        ("w15_out", "w15_in", barrier(15)),
        ("sm_n", "ml_smithy", Gate::Door(Smithy, MillLane)),
        // The Bakery
        ("bk_s", "bk_c", OPEN),
        ("bk_c", "bk_n", OPEN),
        ("bk_s", "bk_sw", OPEN),
        ("bk_sw", "bk_nw", OPEN),
        ("bk_nw", "bk_n", OPEN),
        ("bk_c", "bk_sw", OPEN),
        ("bk_c", "bk_nw", OPEN),
        ("bk_s", "bk_se", OPEN),
        ("bk_se", "bk_e", OPEN),
        ("bk_e", "bk_ne", OPEN),
        ("bk_ne", "bk_n", OPEN),
        ("bk_n", "bl_bakery", Gate::Door(Bakery, BackLane)),
        // The Schoolhouse
        ("sc_s", "sc_c", OPEN),
        ("sc_c", "sc_n", OPEN),
        ("sc_c", "w16_in", OPEN),
        ("sc_c", "w17_in", OPEN),
        ("sc_s", "sc_sw", OPEN),
        ("sc_s", "sc_se", OPEN),
        ("sc_n", "sc_nw", OPEN),
        ("sc_n", "sc_ne", OPEN),
        ("sc_sw", "w16_in", OPEN),
        ("sc_nw", "w16_in", OPEN),
        ("sc_se", "w17_in", OPEN),
        ("sc_ne", "w17_in", OPEN),
        ("w16_out", "w16_in", barrier(16)),
        ("w17_out", "w17_in", barrier(17)),
        ("sc_n", "cl_school", Gate::Door(Schoolhouse, ChurchLane)),
        // The Church
        ("ch_s", "ch_c", OPEN),
        ("ch_c", "ch_n", OPEN),
        ("ch_c", "w18_in", OPEN),
        ("ch_c", "ch_aisle", OPEN),
        ("ch_aisle", "ch_dais", OPEN),
        ("ch_s", "ch_pew_s", OPEN),
        ("ch_pew_s", "ch_se", OPEN),
        ("ch_se", "ch_e_s", OPEN),
        ("ch_n", "ch_pew_n", OPEN),
        ("ch_pew_n", "ch_ne", OPEN),
        ("ch_ne", "ch_e_n", OPEN),
        ("w18_out", "w18_in", barrier(18)),
        ("ch_n", "cl_church", Gate::Door(Church, ChurchLane)),
        // The Back Lane, and the Power's gates at either end
        ("bl_w", "bl_a", OPEN),
        ("bl_a", "bl_bakery", OPEN),
        ("bl_a", "bl_b", OPEN),
        ("bl_a", "w19_in", OPEN),
        ("bl_b", "w19_in", OPEN),
        ("bl_b", "bl_c", OPEN),
        ("bl_c", "bl_inn", OPEN),
        ("bl_c", "bl_d", OPEN),
        ("bl_d", "w20_in", OPEN),
        ("bl_d", "bl_f", OPEN),
        ("bl_f", "w20_in", OPEN),
        ("bl_f", "w21_in", OPEN),
        ("bl_f", "bl_e", OPEN),
        ("w19_out", "w19_in", barrier(19)),
        ("w20_out", "w20_in", barrier(20)),
        ("w21_out", "w21_in", barrier(21)),
        ("bl_w", "ml_e", Gate::Door(BackLane, MillLane)),
        ("bl_e", "cl_w", Gate::Door(BackLane, ChurchLane)),
        // Mill Lane, and through its gateway into the Orchard
        ("ml_e", "ml_a", OPEN),
        ("ml_a", "ml_smithy", OPEN),
        ("ml_a", "w22_in", OPEN),
        ("ml_a", "ml_b", OPEN),
        ("ml_b", "ml_c", OPEN),
        ("ml_c", "ml_d", OPEN),
        ("ml_d", "ml_mill", OPEN),
        ("ml_d", "ml_w", OPEN),
        ("ml_w", "w23_in", OPEN),
        ("w22_out", "w22_in", barrier(22)),
        ("w23_out", "w23_in", barrier(23)),
        ("ml_c", "or_gate", OPEN),
        // Church Lane, and through its gateway into the Graveyard
        ("cl_w", "cl_a", OPEN),
        ("cl_a", "cl_school", OPEN),
        ("cl_a", "w24_in", OPEN),
        ("cl_a", "cl_b", OPEN),
        ("cl_b", "cl_c", OPEN),
        ("cl_c", "cl_church", OPEN),
        ("cl_c", "cl_e", OPEN),
        ("cl_e", "w25_in", OPEN),
        ("w24_out", "w24_in", barrier(24)),
        ("w25_out", "w25_in", barrier(25)),
        ("cl_c", "gy_gate", OPEN),
        // The Orchard
        ("or_gate", "or_a2", OPEN),
        ("or_gate", "or_a3", OPEN),
        ("or_a1", "or_a2", OPEN),
        ("or_a2", "or_a3", OPEN),
        ("or_a3", "or_a4", OPEN),
        ("or_b1", "or_b2", OPEN),
        ("or_b2", "or_b3", OPEN),
        ("or_b3", "or_b4", OPEN),
        ("or_c1", "or_c2", OPEN),
        ("or_c2", "or_c3", OPEN),
        ("or_c3", "or_c4", OPEN),
        ("or_a1", "or_b1", OPEN),
        ("or_b1", "or_c1", OPEN),
        ("or_a2", "or_b2", OPEN),
        ("or_b2", "or_c2", OPEN),
        ("or_a3", "or_b3", OPEN),
        ("or_b3", "or_c3", OPEN),
        ("or_a4", "or_b4", OPEN),
        ("or_b4", "or_c4", OPEN),
        ("or_b1", "w26_in", OPEN),
        ("or_c2", "w27_in", OPEN),
        ("or_c3", "w27_in", OPEN),
        ("or_b4", "w28_in", OPEN),
        ("or_c4", "w28_in", OPEN),
        ("w26_out", "w26_in", barrier(26)),
        ("w27_out", "w27_in", barrier(27)),
        ("w28_out", "w28_in", barrier(28)),
        // The Graveyard
        ("gy_gate", "gy_s", OPEN),
        ("gy_gate", "gw_1", OPEN),
        ("gy_gate", "ge_1", OPEN),
        ("gy_s", "gy_sw", OPEN),
        ("gy_s", "gy_se", OPEN),
        ("gy_sw", "gy_nw", OPEN),
        ("gy_se", "gy_ne", OPEN),
        ("gy_nw", "gy_ne", OPEN),
        ("gy_nw", "w30_in", OPEN),
        ("gy_ne", "w30_in", OPEN),
        ("gw_1", "gw_2", OPEN),
        ("gw_2", "gw_3", OPEN),
        ("gw_3", "w29_in", OPEN),
        ("w29_in", "gw_5", OPEN),
        ("gw_5", "gw_6", OPEN),
        ("gw_2", "gy_sw", OPEN),
        ("gw_5", "gy_nw", OPEN),
        ("ge_1", "ge_2", OPEN),
        ("ge_2", "w31_in", OPEN),
        ("w31_in", "ge_4", OPEN),
        ("ge_4", "ge_5", OPEN),
        ("ge_5", "ge_6", OPEN),
        ("ge_2", "gy_se", OPEN),
        ("ge_5", "gy_ne", OPEN),
        ("w29_out", "w29_in", barrier(29)),
        ("w30_out", "w30_in", barrier(30)),
        ("w31_out", "w31_in", barrier(31)),
        // The Vent drops
        ("vent_bakery", "bk_c", Gate::OneWay),
        ("vent_smithy", "sm_c", Gate::OneWay),
        ("vent_school", "sc_c", Gate::OneWay),
        ("vent_church", "ch_c", Gate::OneWay),
    ]
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maps::map::MapId;

    /// The Village keeps an island to lead a Train round at each stage of a Run: the market cross
    /// in the Market off the Inn, the millstones by the Power, and the mausoleum by Pack-a-Punch.
    /// Each loop is a level lane at least 1.6 m wide with nothing in it.
    #[test]
    fn the_village_keeps_clear_loops_to_train_round() {
        const fn c(x: f32, z: f32) -> Vec2 {
            v(x, z)
        }
        const LOOPS: &[(&str, &[Vec2])] = &[
            ("the market cross", &[c(-4.5, -3.5), c(4.5, -3.5), c(4.5, -12.5), c(-4.5, -12.5)]),
            ("the millstones", &[c(-66.0, 10.0), c(-58.0, 10.0), c(-58.0, 18.0), c(-66.0, 18.0)]),
            ("the mausoleum", &[c(50.5, 37.5), c(61.5, 37.5), c(61.5, 48.5), c(50.5, 48.5)]),
        ];
        let spoilt = crate::maps::map::tests::spoilt_loops(MapId::Village, 0.0, LOOPS);
        assert!(spoilt.is_empty(), "{spoilt:#?}");
    }

    /// The market cross is a real island, not a post: its base is wide enough that a Train led
    /// round it stays a body's length behind, and too tall to climb onto.
    #[test]
    fn the_market_cross_stands_on_a_base_too_wide_to_cut_round_and_too_tall_to_climb() {
        const { assert!(MARKET_CROSS_BASE >= 4.0) };
        const { assert!(MARKET_CROSS_BASE_HEIGHT > 1.3, "a Player could clamber onto the market cross's base") };
    }
}
