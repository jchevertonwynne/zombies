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
//! The whole Map stands `VILLAGE_X` along x from the world's middle, clear of the others.

use bevy::prelude::*;

use crate::enemies::nav::Gate;
use crate::maps::map::BarrierId;
use crate::maps::zone::ZoneId;
use crate::tuning::*;
use crate::weapons::arsenal::WeaponId;

use super::build::{Builder, Gap};
use super::{DOOR_HEIGHT, Layout, Prop, PropKind, RaisedFloor, STOREY, Slope, door, palette, prop};

/// How far along x the Village stands from the world's middle, clear of the Greybox and the
/// Quarry.
pub const VILLAGE_X: f32 = -200.0;
const V: f32 = VILLAGE_X;

/// The first of the Village's windows in `WINDOWS`, after the Greybox's and the Quarry's.
const W0: u8 = (super::greybox::WINDOWS.len() + super::quarry::WINDOWS.len()) as u8;

/// How high a chalk outline's centre sits on its wall.
const CHALK: f32 = 1.5;

/// A point in the Village, `x` along from its middle.
const fn at(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(V + x, y, z)
}

/// A spot on a wall or a floor's plan in the Village, `x` along from its middle.
const fn v(x: f32, z: f32) -> Vec2 {
    Vec2::new(V + x, z)
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
    hellhound_spawns: &HELLHOUND_SPAWNS,
    special_spawns: &SPECIAL_SPAWNS,
    vents: &VENTS,
    raised_floors: &[CHANCEL],
    slopes: &SLOPES,
    sunken: &[],
    props: PROPS,
    lamps: &LAMPS,
    fixed_joins: &FIXED_JOINS,
    nav_nodes: NODES,
    nav_edges: EDGES,
    jump_pads: &[],
    sun: Vec3::new(-1.0, -3.0, 1.5),
    plan: (Vec2::new(V - 80.0, -30.0), Vec2::new(V + 80.0, 60.0)),
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

/// The lamps: one in each building, and lamps along the streets and lanes.
const LAMPS: [(ZoneId, Vec3); 21] = [
    (ZoneId::Inn, at(0.0, STOREY - 0.5, 14.0)),
    (ZoneId::Bakery, at(-16.0, STOREY - 0.5, 14.0)),
    (ZoneId::Smithy, at(-27.0, STOREY - 0.5, 14.0)),
    (ZoneId::Mill, at(-62.0, STOREY - 0.5, 9.0)),
    (ZoneId::Schoolhouse, at(32.0, STOREY - 0.5, 14.0)),
    (ZoneId::Church, at(62.0, STOREY - 0.5, 14.0)),
    (ZoneId::Market, at(-10.0, STOREY + 1.0, -8.0)),
    (ZoneId::Market, at(10.0, STOREY + 1.0, -8.0)),
    (ZoneId::Market, at(0.0, STOREY + 1.0, -18.0)),
    (ZoneId::WestEnd, at(-36.0, STOREY + 0.5, 0.0)),
    (ZoneId::WestEnd, at(-60.0, STOREY + 0.5, 0.0)),
    (ZoneId::EastEnd, at(36.0, STOREY + 0.5, 0.0)),
    (ZoneId::EastEnd, at(60.0, STOREY + 0.5, 0.0)),
    (ZoneId::BackLane, at(0.0, STOREY + 0.5, 26.0)),
    (ZoneId::MillLane, at(-48.0, STOREY + 0.5, 26.0)),
    (ZoneId::ChurchLane, at(48.0, STOREY + 0.5, 26.0)),
    (ZoneId::Orchard, at(-62.0, STOREY + 1.5, 36.0)),
    (ZoneId::Orchard, at(-50.0, STOREY + 1.5, 48.0)),
    (ZoneId::Graveyard, at(48.0, STOREY + 1.5, 36.0)),
    (ZoneId::Graveyard, at(64.0, STOREY + 1.5, 36.0)),
    (ZoneId::Graveyard, at(56.0, STOREY + 1.5, 49.0)),
];

/// Old dark stone, for the chancel and its steps.
const CHANCEL_STONE: Color = Color::srgb(0.38, 0.35, 0.33);

/// The Church's chancel: a dais across its east end, Pack-a-Punch at its south end and the altar
/// against the east wall, up a flight of steps from the aisle.
const CHANCEL: RaisedFloor = RaisedFloor { zone: ZoneId::Church, min: v(66.0, 8.0), max: v(71.8, 20.0), height: 0.45, colour: CHANCEL_STONE };

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
/// round are the Market's cross, the Mill's millstones and the Graveyard's mausoleum (built with
/// the walls).
const PROPS: &[Prop] = {
    use PropKind::*;
    use ZoneId::*;
    &[
        // The Inn: the bar along its wall to the Bakery, and two tables.
        prop(Counter, Inn, (V - 7.4, 0.0, 14.0), (0.8, 1.1, 5.0)),
        prop(Table, Inn, (V - 3.5, 0.0, 9.5), (1.4, 0.76, 1.4)),
        prop(Table, Inn, (V - 3.5, 0.0, 18.5), (1.4, 0.76, 1.4)),
        // The Market: the market cross in the middle, and a stall either side.
        prop(Pillar, Market, (V, 0.0, -8.0), (1.2, 3.5, 1.2)),
        prop(Counter, Market, (V - 14.0, 0.0, -16.0), (2.6, 1.0, 1.0)),
        prop(Counter, Market, (V + 14.0, 0.0, -16.0), (2.6, 1.0, 1.0)),
        // The West End and the East End: a cart against the south side, and drums against the
        // garden walls.
        prop(Machine, WestEnd, (V - 42.0, 0.0, -5.1), (3.0, 1.4, 1.4)),
        prop(Barrel, WestEnd, (V - 46.0, 0.0, 5.4), (0.65, 0.95, 0.65)),
        prop(Barrel, WestEnd, (V - 45.2, 0.0, 5.45), (0.65, 0.95, 0.65)),
        prop(Machine, EastEnd, (V + 42.0, 0.0, -5.1), (3.0, 1.4, 1.4)),
        prop(Barrel, EastEnd, (V + 46.0, 0.0, 5.4), (0.65, 0.95, 0.65)),
        // The lanes: drums and a crate against their north walls.
        prop(Barrel, BackLane, (V + 20.5, 0.0, 29.4), (0.65, 0.95, 0.65)),
        prop(Barrel, BackLane, (V + 21.3, 0.0, 29.45), (0.65, 0.95, 0.65)),
        prop(Crate, MillLane, (V - 28.5, 0.0, 29.3), (1.0, 0.8, 1.0)),
        prop(Barrel, ChurchLane, (V + 28.6, 0.0, 29.4), (0.65, 0.95, 0.65)),
        // The Mill: the millstones in the middle, and sacks in a corner.
        prop(Machine, Mill, (V - 62.0, 0.0, 14.0), (3.0, 1.3, 3.0)),
        prop(Crate, Mill, (V - 52.7, 0.0, 21.3), (1.0, 0.8, 1.0)),
        // The Smithy: the forge against its east wall, and the anvil before it.
        prop(Machine, Smithy, (V - 25.2, 0.0, 14.0), (2.0, 1.6, 2.4)),
        prop(Machine, Smithy, (V - 29.0, 0.0, 14.0), (0.6, 0.8, 1.0)),
        // The Bakery: the oven against its west wall, the kneading table, and flour sacks.
        prop(Machine, Bakery, (V - 22.8, 0.0, 14.0), (2.0, 2.0, 3.0)),
        prop(Table, Bakery, (V - 14.5, 0.0, 14.0), (2.4, 0.9, 1.0)),
        prop(Sandbags, Bakery, (V - 10.0, 0.0, 21.4), (2.0, 0.6, 0.8)),
        // The Schoolhouse: two rows of desks either side of the aisle.
        prop(Table, Schoolhouse, (V + 28.5, 0.0, 10.5), (3.0, 0.76, 0.8)),
        prop(Table, Schoolhouse, (V + 35.5, 0.0, 10.5), (3.0, 0.76, 0.8)),
        prop(Table, Schoolhouse, (V + 28.5, 0.0, 17.5), (3.0, 0.76, 0.8)),
        prop(Table, Schoolhouse, (V + 35.5, 0.0, 17.5), (3.0, 0.76, 0.8)),
        // The Church: four pews either side of the aisle, and the altar up on the chancel.
        prop(Bench, Church, (V + 60.5, 0.0, 10.5), (0.6, 0.5, 3.0)),
        prop(Bench, Church, (V + 63.0, 0.0, 10.5), (0.6, 0.5, 3.0)),
        prop(Bench, Church, (V + 60.5, 0.0, 17.5), (0.6, 0.5, 3.0)),
        prop(Bench, Church, (V + 63.0, 0.0, 17.5), (0.6, 0.5, 3.0)),
        prop(Altar, Church, (V + 71.3, CHANCEL.height, 15.5), (0.9, 1.0, 2.4)),
        // The Orchard: two staggered rows of trees.
        prop(Tree, Orchard, (V - 64.0, 0.0, 37.5), (0.5, 3.2, 0.5)),
        prop(Tree, Orchard, (V - 56.0, 0.0, 37.5), (0.5, 3.2, 0.5)),
        prop(Tree, Orchard, (V - 48.0, 0.0, 37.5), (0.5, 3.2, 0.5)),
        prop(Tree, Orchard, (V - 68.0, 0.0, 46.5), (0.5, 3.2, 0.5)),
        prop(Tree, Orchard, (V - 60.0, 0.0, 46.5), (0.5, 3.2, 0.5)),
        prop(Tree, Orchard, (V - 52.0, 0.0, 46.5), (0.5, 3.2, 0.5)),
        prop(Tree, Orchard, (V - 44.0, 0.0, 46.5), (0.5, 3.2, 0.5)),
        // The Graveyard: rows of graves either side of the mausoleum.
        prop(Headstone, Graveyard, (V + 44.0, 0.0, 35.0), HEADSTONE),
        prop(Headstone, Graveyard, (V + 47.0, 0.0, 35.0), HEADSTONE),
        prop(Headstone, Graveyard, (V + 44.0, 0.0, 39.0), HEADSTONE),
        prop(Headstone, Graveyard, (V + 47.0, 0.0, 39.0), HEADSTONE),
        prop(Headstone, Graveyard, (V + 44.0, 0.0, 43.0), HEADSTONE),
        prop(Headstone, Graveyard, (V + 47.0, 0.0, 43.0), HEADSTONE),
        prop(Headstone, Graveyard, (V + 44.0, 0.0, 47.0), HEADSTONE),
        prop(Headstone, Graveyard, (V + 47.0, 0.0, 47.0), HEADSTONE),
        prop(Headstone, Graveyard, (V + 44.0, 0.0, 51.0), HEADSTONE),
        prop(Headstone, Graveyard, (V + 47.0, 0.0, 51.0), HEADSTONE),
        prop(Headstone, Graveyard, (V + 65.0, 0.0, 35.0), HEADSTONE),
        prop(Headstone, Graveyard, (V + 68.0, 0.0, 35.0), HEADSTONE),
        prop(Headstone, Graveyard, (V + 65.0, 0.0, 39.0), HEADSTONE),
        prop(Headstone, Graveyard, (V + 68.0, 0.0, 39.0), HEADSTONE),
        prop(Headstone, Graveyard, (V + 65.0, 0.0, 43.0), HEADSTONE),
        prop(Headstone, Graveyard, (V + 68.0, 0.0, 43.0), HEADSTONE),
        prop(Headstone, Graveyard, (V + 65.0, 0.0, 47.0), HEADSTONE),
        prop(Headstone, Graveyard, (V + 68.0, 0.0, 47.0), HEADSTONE),
        prop(Headstone, Graveyard, (V + 65.0, 0.0, 51.0), HEADSTONE),
        prop(Headstone, Graveyard, (V + 68.0, 0.0, 51.0), HEADSTONE),
    ]
};

/// A headstone's size: wide across the row, waist-high and thin.
const HEADSTONE: (f32, f32, f32) = (0.8, 0.9, 0.25);

/// The mausoleum in the middle of the Graveyard: its footprint's corners (x, z) and how tall it is.
const MAUSOLEUM: (Vec2, Vec2, f32) = (v(53.0, 40.5), v(59.0, 45.5), 3.5);

/// The walls, the Doors, the gates, the ground round the village and its landmarks.
fn build(b: &mut Builder) {
    use ZoneId::*;
    let outside = palette(None);

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
        b.block(Vec3::new(size.x, 0.2, size.y), Vec3::new(mid.x, -0.1, mid.y), outside.floor);
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
    b.block(Vec3::new(size.x, tall, size.y), Vec3::new(mid.x, tall / 2.0, mid.y), stone);
    b.block(Vec3::new(size.x + 0.4, 0.4, size.y + 0.4), Vec3::new(mid.x, tall + 0.2, mid.y), palette(Some(Graveyard)).trim);

    // The church tower, in the garden by the Church, standing over the whole village.
    let tower = palette(Some(Church)).wall;
    b.block(Vec3::new(4.0, 14.0, 4.0), at(46.0, 7.0, 19.5), tower);
    b.block(Vec3::new(4.6, 0.6, 4.6), at(46.0, 14.3, 19.5), palette(Some(Church)).trim);
    // The Mill's wheel, on its west end, out over the verge.
    b.block(Vec3::new(0.6, 6.0, 6.0), at(-74.5, 2.0, 14.0), palette(Some(Mill)).trim);
}

/// The path nodes: a name, then where a Zombie's feet stand there (x, y, z).
const NODES: &[(&str, f32, f32, f32)] = &[
    // The Inn, round its tables and along its bar
    ("inn", V, 0.0, 14.0),
    ("inn_s", V, 0.0, 7.5),
    ("inn_n", V, 0.0, 20.5),
    ("inn_w", V - 5.5, 0.0, 14.0),
    ("inn_sw", V - 6.0, 0.0, 7.5),
    ("inn_nw", V - 6.0, 0.0, 20.5),
    ("inn_e", V + 5.5, 0.0, 14.0),
    ("w0_in", V + 6.5, 0.0, 10.0),
    ("w0_out", V + 10.0, 0.0, 10.0),
    ("w1_in", V + 6.5, 0.0, 18.0),
    ("w1_out", V + 10.0, 0.0, 18.0),
    // The Market, round the cross
    ("mk_inn", V, 0.0, 4.0),
    ("mk_bakery", V - 16.0, 0.0, 4.0),
    ("mk_wdoor", V - 22.5, 0.0, 0.0),
    ("mk_edoor", V + 22.5, 0.0, 0.0),
    ("mk_nw", V - 16.0, 0.0, -3.0),
    ("mk_ne", V + 16.0, 0.0, -3.0),
    ("mk_cn", V, 0.0, -3.0),
    ("mk_cw", V - 5.0, 0.0, -8.0),
    ("mk_ce", V + 5.0, 0.0, -8.0),
    ("mk_cs", V, 0.0, -13.0),
    ("mk_w", V - 20.0, 0.0, -11.0),
    ("mk_e", V + 20.0, 0.0, -11.0),
    ("mk_sw", V - 19.0, 0.0, -19.0),
    ("mk_s", V, 0.0, -19.0),
    ("mk_se", V + 19.0, 0.0, -19.0),
    ("w2_in", V - 12.0, 0.0, -20.5),
    ("w2_out", V - 12.0, 0.0, -24.0),
    ("w3_in", V + 12.0, 0.0, -20.5),
    ("w3_out", V + 12.0, 0.0, -24.0),
    ("w4_in", V - 22.5, 0.0, -14.0),
    ("w4_out", V - 26.0, 0.0, -14.0),
    ("w5_in", V + 22.5, 0.0, -14.0),
    ("w5_out", V + 26.0, 0.0, -14.0),
    ("w6_in", V + 16.0, 0.0, 4.5),
    ("w6_out", V + 16.0, 0.0, 8.0),
    // The West End
    ("we_door", V - 25.5, 0.0, 0.0),
    ("we_1", V - 32.0, 0.0, 0.0),
    ("we_smithy", V - 32.0, 0.0, 4.5),
    ("we_2", V - 42.0, 0.0, 0.0),
    ("we_3", V - 52.0, 0.0, 0.0),
    ("we_4", V - 62.0, 0.0, 0.0),
    ("we_mill", V - 62.0, 0.0, 4.5),
    ("we_w", V - 68.0, 0.0, 0.0),
    ("w7_in", V - 60.0, 0.0, -4.5),
    ("w7_out", V - 60.0, 0.0, -8.0),
    ("w8_in", V - 36.0, 0.0, -4.5),
    ("w8_out", V - 36.0, 0.0, -8.0),
    ("w9_in", V - 70.5, 0.0, 0.0),
    ("w9_out", V - 74.0, 0.0, 0.0),
    // The East End
    ("ee_door", V + 25.5, 0.0, 0.0),
    ("ee_1", V + 32.0, 0.0, 0.0),
    ("ee_school", V + 32.0, 0.0, 4.5),
    ("ee_2", V + 42.0, 0.0, 0.0),
    ("ee_3", V + 50.0, 0.0, 0.0),
    ("ee_4", V + 58.0, 0.0, 0.0),
    ("ee_church", V + 58.0, 0.0, 4.5),
    ("ee_e", V + 68.0, 0.0, 0.0),
    ("w10_in", V + 36.0, 0.0, -4.5),
    ("w10_out", V + 36.0, 0.0, -8.0),
    ("w11_in", V + 60.0, 0.0, -4.5),
    ("w11_out", V + 60.0, 0.0, -8.0),
    ("w12_in", V + 70.5, 0.0, 0.0),
    ("w12_out", V + 74.0, 0.0, 0.0),
    // The Mill, round the millstones
    ("mill_s", V - 62.0, 0.0, 7.5),
    ("mill_n", V - 62.0, 0.0, 20.5),
    ("mill_sw", V - 68.0, 0.0, 9.0),
    ("mill_nw", V - 68.0, 0.0, 19.0),
    ("mill_se", V - 56.0, 0.0, 9.0),
    ("mill_ne", V - 56.0, 0.0, 19.0),
    ("w13_in", V - 70.5, 0.0, 14.0),
    ("w13_out", V - 74.0, 0.0, 14.0),
    ("w14_in", V - 53.5, 0.0, 14.0),
    ("w14_out", V - 50.0, 0.0, 14.0),
    // The Smithy, round the anvil
    ("sm_s", V - 32.0, 0.0, 7.5),
    ("sm_n", V - 32.0, 0.0, 20.5),
    ("sm_c", V - 32.0, 0.0, 14.0),
    ("sm_sw", V - 38.0, 0.0, 8.5),
    ("sm_nw", V - 38.0, 0.0, 19.0),
    ("sm_se", V - 27.0, 0.0, 8.5),
    ("sm_ne", V - 27.0, 0.0, 19.5),
    ("w15_in", V - 38.5, 0.0, 14.0),
    ("w15_out", V - 42.0, 0.0, 14.0),
    // The Bakery, round the kneading table
    ("bk_s", V - 16.0, 0.0, 7.5),
    ("bk_n", V - 16.0, 0.0, 20.5),
    ("bk_c", V - 18.0, 0.0, 14.0),
    ("bk_sw", V - 20.5, 0.0, 8.0),
    ("bk_nw", V - 20.5, 0.0, 20.0),
    ("bk_se", V - 10.5, 0.0, 8.0),
    ("bk_e", V - 11.0, 0.0, 14.0),
    ("bk_ne", V - 11.0, 0.0, 19.5),
    // The Schoolhouse, down its aisle and round the desks
    ("sc_s", V + 32.0, 0.0, 7.5),
    ("sc_n", V + 32.0, 0.0, 20.5),
    ("sc_c", V + 32.0, 0.0, 14.0),
    ("sc_sw", V + 25.6, 0.0, 8.2),
    ("sc_nw", V + 25.6, 0.0, 19.8),
    ("sc_se", V + 38.4, 0.0, 8.2),
    ("sc_ne", V + 38.4, 0.0, 19.8),
    ("w16_in", V + 25.5, 0.0, 14.0),
    ("w16_out", V + 22.0, 0.0, 14.0),
    ("w17_in", V + 38.5, 0.0, 14.0),
    ("w17_out", V + 42.0, 0.0, 14.0),
    // The Church: the nave, the aisle to the chancel steps, and up on the chancel
    ("ch_s", V + 58.0, 0.0, 7.5),
    ("ch_n", V + 58.0, 0.0, 20.5),
    ("ch_c", V + 58.0, 0.0, 14.0),
    ("ch_pew_s", V + 61.75, 0.0, 7.6),
    ("ch_pew_n", V + 61.75, 0.0, 20.4),
    ("ch_se", V + 65.0, 0.0, 7.2),
    ("ch_ne", V + 65.0, 0.0, 20.8),
    ("ch_e_s", V + 70.8, 0.0, 7.1),
    ("ch_e_n", V + 70.8, 0.0, 20.9),
    ("ch_aisle", V + 64.2, 0.0, 14.0),
    ("ch_dais", V + 67.8, CHANCEL.height, 14.0),
    ("w18_in", V + 53.5, 0.0, 14.0),
    ("w18_out", V + 50.0, 0.0, 14.0),
    // The Back Lane
    ("bl_w", V - 22.5, 0.0, 26.0),
    ("bl_a", V - 16.0, 0.0, 26.0),
    ("bl_bakery", V - 16.0, 0.0, 23.5),
    ("bl_b", V - 8.0, 0.0, 26.0),
    ("bl_c", V, 0.0, 26.0),
    ("bl_inn", V, 0.0, 23.5),
    ("bl_d", V + 8.0, 0.0, 26.0),
    ("bl_f", V + 16.0, 0.0, 26.0),
    ("bl_e", V + 22.5, 0.0, 26.0),
    ("w19_in", V - 12.0, 0.0, 28.5),
    ("w19_out", V - 12.0, 0.0, 32.0),
    ("w20_in", V + 12.0, 0.0, 28.5),
    ("w20_out", V + 12.0, 0.0, 32.0),
    ("w21_in", V + 16.0, 0.0, 23.5),
    ("w21_out", V + 16.0, 0.0, 20.0),
    // Mill Lane
    ("ml_e", V - 25.5, 0.0, 26.0),
    ("ml_a", V - 32.0, 0.0, 26.0),
    ("ml_smithy", V - 32.0, 0.0, 23.5),
    ("ml_b", V - 44.0, 0.0, 26.0),
    ("ml_c", V - 56.0, 0.0, 26.0),
    ("ml_d", V - 62.0, 0.0, 26.0),
    ("ml_mill", V - 62.0, 0.0, 23.5),
    ("ml_w", V - 68.0, 0.0, 26.0),
    ("w22_in", V - 32.0, 0.0, 28.5),
    ("w22_out", V - 32.0, 0.0, 32.0),
    ("w23_in", V - 70.5, 0.0, 26.0),
    ("w23_out", V - 74.0, 0.0, 26.0),
    // Church Lane
    ("cl_w", V + 25.5, 0.0, 26.0),
    ("cl_a", V + 32.0, 0.0, 26.0),
    ("cl_school", V + 32.0, 0.0, 23.5),
    ("cl_b", V + 44.0, 0.0, 26.0),
    ("cl_c", V + 56.0, 0.0, 26.0),
    ("cl_church", V + 58.0, 0.0, 23.5),
    ("cl_e", V + 68.0, 0.0, 26.0),
    ("w24_in", V + 32.0, 0.0, 28.5),
    ("w24_out", V + 32.0, 0.0, 32.0),
    ("w25_in", V + 70.5, 0.0, 26.0),
    ("w25_out", V + 74.0, 0.0, 26.0),
    // The Orchard, in a grid between the rows of trees
    ("or_gate", V - 56.0, 0.0, 32.0),
    ("or_a1", V - 66.0, 0.0, 33.0),
    ("or_a2", V - 58.0, 0.0, 33.0),
    ("or_a3", V - 50.0, 0.0, 33.0),
    ("or_a4", V - 42.0, 0.0, 33.0),
    ("or_b1", V - 66.0, 0.0, 42.0),
    ("or_b2", V - 58.0, 0.0, 42.0),
    ("or_b3", V - 50.0, 0.0, 42.0),
    ("or_b4", V - 42.0, 0.0, 42.0),
    ("or_c1", V - 66.0, 0.0, 51.0),
    ("or_c2", V - 58.0, 0.0, 51.0),
    ("or_c3", V - 50.0, 0.0, 51.0),
    ("or_c4", V - 42.0, 0.0, 51.0),
    ("w26_in", V - 70.5, 0.0, 42.0),
    ("w26_out", V - 74.0, 0.0, 42.0),
    ("w27_in", V - 56.0, 0.0, 52.5),
    ("w27_out", V - 56.0, 0.0, 56.0),
    ("w28_in", V - 41.5, 0.0, 44.0),
    ("w28_out", V - 38.0, 0.0, 44.0),
    // The Graveyard: round the mausoleum, and down the paths outside the rows of graves
    ("gy_gate", V + 56.0, 0.0, 32.0),
    ("gy_s", V + 56.0, 0.0, 36.3),
    ("gy_sw", V + 51.0, 0.0, 37.8),
    ("gy_se", V + 61.0, 0.0, 37.8),
    ("gy_nw", V + 51.0, 0.0, 47.8),
    ("gy_ne", V + 61.0, 0.0, 47.8),
    ("gw_1", V + 42.0, 0.0, 33.0),
    ("gw_2", V + 42.0, 0.0, 37.0),
    ("gw_3", V + 42.0, 0.0, 41.0),
    ("gw_5", V + 42.0, 0.0, 49.0),
    ("gw_6", V + 42.0, 0.0, 52.8),
    ("ge_1", V + 70.0, 0.0, 33.0),
    ("ge_2", V + 70.0, 0.0, 37.0),
    ("ge_4", V + 70.0, 0.0, 45.0),
    ("ge_5", V + 70.0, 0.0, 49.0),
    ("ge_6", V + 70.0, 0.0, 52.8),
    ("w29_in", V + 41.5, 0.0, 44.0),
    ("w29_out", V + 38.0, 0.0, 44.0),
    ("w30_in", V + 56.0, 0.0, 52.5),
    ("w30_out", V + 56.0, 0.0, 56.0),
    ("w31_in", V + 70.5, 0.0, 42.0),
    ("w31_out", V + 74.0, 0.0, 42.0),
    // Vents, up near the rafters, over the spot a Nova Crawler drops to
    ("vent_bakery", V - 18.0, NOVA_VENT_HEIGHT, 14.0),
    ("vent_smithy", V - 32.0, NOVA_VENT_HEIGHT, 14.0),
    ("vent_school", V + 32.0, NOVA_VENT_HEIGHT, 14.0),
    ("vent_church", V + 58.0, NOVA_VENT_HEIGHT, 14.0),
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
