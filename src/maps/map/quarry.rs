//! The Quarry Map: an open-cast pit and the works built up its terraced side, climbed a storey
//! at a time. A Run starts in the Pump House at the foot of the pit and opens onto the Quarry
//! Floor, an open bowl with the sump pond in it and Zombies rising from the gravel. A ramp climbs
//! the east side to the Rail Yard on the first terrace, beside the Crusher Shed; steps climb from
//! there to the Winding House and the Offices on the second, and the Headframe stands over them,
//! with Pack-a-Punch up on its deck.
//!
//! Going up costs Points and stairs; coming down is a drop off a Ledge, from the Rail Yard into
//! the pit or off the Headframe's deck, which Zombies can follow but never climb back. Once the
//! Power is on, Jump Pads throw a Player back up: from the pit floor to the Rail Yard, and from
//! the foot of the Headframe to its deck. The Quarry has no Teleporter.
//!
//! The whole Map stands `QUARRY_X` along x from the world's middle, clear of the Greybox.

use bevy::prelude::*;

use crate::weapons::arsenal::WeaponId;
use crate::maps::jump_pad::JumpPad;
use crate::maps::map::BarrierId;
use crate::enemies::nav::Gate;
use crate::tuning::*;
use crate::maps::zone::ZoneId;

use super::build::{Builder, Gap};
use super::{DOOR_HEIGHT, DOOR_WIDTH, Layout, PARAPET_THICKNESS, Prop, PropKind, RAIL_HEIGHT, STOREY, Slope, Sunken, door, palette, prop};

/// How far along x the Quarry stands from the world's middle, clear of the Greybox.
pub const QUARRY_X: f32 = 200.0;
const Q: f32 = QUARRY_X;

/// The first terrace's height, and the second's, and the Headframe's deck.
const TERRACE_1: f32 = STOREY;
const TERRACE_2: f32 = 2.0 * STOREY;
const DECK: f32 = 3.0 * STOREY;

/// Where the ramp up from the pit floor and the steps up from the Rail Yard run (x, before
/// `QUARRY_X`): both against the Map's east wall, each with a Door at its top.
const CLIMB_X: f32 = 18.3;
/// Where the Headframe's stairs run up its west wall (x, before `QUARRY_X`).
const STAIR_X: f32 = -4.3;

/// The first of the Quarry's windows in `WINDOWS`, after the Greybox's.
const W0: u8 = super::greybox::WINDOWS.len() as u8;

/// A point in the Quarry, `x` along from its middle.
const fn at(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(Q + x, y, z)
}

/// A spot on a wall or a floor's plan in the Quarry, `x` along from its middle.
const fn v(x: f32, z: f32) -> Vec2 {
    Vec2::new(Q + x, z)
}

const fn barrier(i: u8) -> Gate {
    Gate::Barrier(BarrierId(W0 + i))
}

pub(super) const QUARRY: Layout = Layout {
    zones: &{
        use ZoneId::*;
        [PumpHouse, QuarryFloor, RailYard, CrusherShed, WindingHouse, Offices, Headframe]
    },
    // In the Pump House, clear of its Box, its windows and the Door out.
    seats: [at(0.0, 0.0, -26.0), at(-3.0, 0.0, -26.0), at(3.0, 0.0, -26.0), at(0.0, 0.0, -23.0)],
    wall_weapons: &WALL_WEAPONS,
    windows: &WINDOWS,
    risers: &RISERS,
    hellhound_spawns: &HELLHOUND_SPAWNS,
    special_spawns: &SPECIAL_SPAWNS,
    vents: &VENTS,
    raised_floors: &[],
    slopes: &SLOPES,
    sunken: &[SUMP],
    props: PROPS,
    lamps: &LAMPS,
    fixed_joins: &FIXED_JOINS,
    nav_nodes: NODES,
    nav_edges: EDGES,
    jump_pads: &JUMP_PADS,
    sun: Vec3::new(1.0, -3.0, -1.5),
    plan: (Vec2::new(Q - 28.0, -38.0), Vec2::new(Q + 28.0, 44.0)),
    build,
};

/// The chalk outlines. The Quarry has all five Wall Weapons, the dearer the higher up.
const WALL_WEAPONS: [(WeaponId, ZoneId, Vec3, Vec2); 5] = [
    // Either side of the Pump House's Door, facing the Players at the start.
    (WeaponId::M14, ZoneId::PumpHouse, at(-5.0, 1.5, -20.2), Vec2::NEG_Y),
    (WeaponId::Olympia, ZoneId::PumpHouse, at(5.0, 1.5, -20.2), Vec2::NEG_Y),
    // On the face of the terrace, at the pit floor's far end.
    (WeaponId::Mp40, ZoneId::QuarryFloor, at(-14.0, 1.5, 1.8), Vec2::NEG_Y),
    // On the Crusher Shed's outside wall, facing the Rail Yard.
    (WeaponId::TrenchGun, ZoneId::RailYard, at(0.2, TERRACE_1 + 1.5, 13.0), Vec2::X),
    // Inside the Crusher Shed, on its south wall over the pit.
    (WeaponId::Ak74u, ZoneId::CrusherShed, at(-9.0, TERRACE_1 + 1.5, 2.2), Vec2::Y),
];

/// The Box Locations, in order. Every Run starts the Box at the first.
pub(super) const BOX_LOCATIONS: [(ZoneId, Vec3, Vec2); 5] = [
    // Against the Pump House's back wall, between its windows.
    (ZoneId::PumpHouse, at(0.0, 0.0, -29.35), Vec2::Y),
    // Against the pit's west wall, past the sump.
    (ZoneId::QuarryFloor, at(-19.35, 0.0, -12.0), Vec2::X),
    // Against the Crusher Shed's south wall.
    (ZoneId::CrusherShed, at(-5.0, TERRACE_1, 2.65), Vec2::Y),
    // In the Offices' south-west corner.
    (ZoneId::Offices, at(-17.0, TERRACE_2, 16.65), Vec2::Y),
    // Under the Headframe's deck, against its east wall.
    (ZoneId::Headframe, at(5.35, TERRACE_2, 37.0), Vec2::NEG_X),
];

/// The 12 windows: three in the Pump House, an adit in each of the pit's side walls, and the
/// rest on the terraces, each over a ledge of rock outside where Zombies gather.
pub(super) const WINDOWS: [(ZoneId, Vec3, Vec2); 12] = [
    (ZoneId::PumpHouse, at(-4.0, 0.0, -30.0), Vec2::NEG_Y),
    (ZoneId::PumpHouse, at(4.0, 0.0, -30.0), Vec2::NEG_Y),
    (ZoneId::PumpHouse, at(-8.0, 0.0, -25.0), Vec2::NEG_X),
    (ZoneId::QuarryFloor, at(-20.0, 0.0, -4.0), Vec2::NEG_X),
    (ZoneId::QuarryFloor, at(20.0, 0.0, -14.0), Vec2::X),
    (ZoneId::RailYard, at(20.0, TERRACE_1, 4.5), Vec2::X),
    (ZoneId::CrusherShed, at(-20.0, TERRACE_1, 5.0), Vec2::NEG_X),
    (ZoneId::CrusherShed, at(-20.0, TERRACE_1, 12.0), Vec2::NEG_X),
    (ZoneId::WindingHouse, at(20.0, TERRACE_2, 22.0), Vec2::X),
    (ZoneId::WindingHouse, at(14.0, TERRACE_2, 28.0), Vec2::Y),
    (ZoneId::Offices, at(-20.0, TERRACE_2, 22.0), Vec2::NEG_X),
    (ZoneId::Offices, at(-12.0, TERRACE_2, 28.0), Vec2::Y),
];

/// The Risers: on the pit floor, clear of the sump and the ramp, and in the Rail Yard.
const RISERS: [(ZoneId, Vec3); 6] = [
    (ZoneId::QuarryFloor, at(-4.0, 0.0, -15.0)),
    (ZoneId::QuarryFloor, at(12.0, 0.0, -15.0)),
    (ZoneId::QuarryFloor, at(-14.0, 0.0, -1.0)),
    (ZoneId::QuarryFloor, at(11.0, 0.0, -1.0)),
    (ZoneId::RailYard, at(4.0, TERRACE_1, 12.0)),
    (ZoneId::RailYard, at(12.0, TERRACE_1, 10.0)),
];

/// The Hellhound Spawn Points, two in every Zone.
const HELLHOUND_SPAWNS: [(ZoneId, Vec3); 15] = [
    (ZoneId::PumpHouse, at(-5.0, 0.0, -22.5)),
    (ZoneId::PumpHouse, at(4.5, 0.0, -22.5)),
    (ZoneId::QuarryFloor, at(-2.0, 0.0, -5.0)),
    (ZoneId::QuarryFloor, at(12.0, 0.0, -6.0)),
    (ZoneId::QuarryFloor, at(-14.0, 0.0, -18.5)),
    (ZoneId::RailYard, at(12.0, TERRACE_1, 5.5)),
    (ZoneId::RailYard, at(6.0, TERRACE_1, 11.0)),
    (ZoneId::CrusherShed, at(-15.0, TERRACE_1, 6.0)),
    (ZoneId::CrusherShed, at(-4.0, TERRACE_1, 13.0)),
    (ZoneId::WindingHouse, at(16.0, TERRACE_2, 25.0)),
    (ZoneId::WindingHouse, at(4.0, TERRACE_2, 19.5)),
    (ZoneId::Offices, at(-3.0, TERRACE_2, 18.5)),
    (ZoneId::Offices, at(-16.0, TERRACE_2, 21.0)),
    (ZoneId::Headframe, at(4.5, TERRACE_2, 31.0)),
    (ZoneId::Headframe, at(3.5, DECK, 38.6)),
];

/// The Special Spawn Points, one in each of the bigger Zones.
const SPECIAL_SPAWNS: [(ZoneId, Vec3); 6] = [
    (ZoneId::QuarryFloor, at(-2.0, 0.0, -5.0)),
    (ZoneId::QuarryFloor, at(12.0, 0.0, -6.0)),
    (ZoneId::RailYard, at(12.0, TERRACE_1, 5.5)),
    (ZoneId::CrusherShed, at(-15.0, TERRACE_1, 6.0)),
    (ZoneId::WindingHouse, at(16.0, TERRACE_2, 25.0)),
    (ZoneId::Offices, at(-3.0, TERRACE_2, 18.5)),
];

/// The Vents, in the three buildings up on the terraces.
const VENTS: [(ZoneId, Vec3); 3] = [
    (ZoneId::CrusherShed, at(-10.0, TERRACE_1, 4.5)),
    (ZoneId::WindingHouse, at(15.0, TERRACE_2, 19.0)),
    (ZoneId::Offices, at(-14.0, TERRACE_2, 25.0)),
];

/// The drops off the Rail Yard's edge into the pit, always open: one-way to walk, but they make
/// each of the two Zones Active from the other.
const FIXED_JOINS: [(ZoneId, ZoneId); 1] = [(ZoneId::RailYard, ZoneId::QuarryFloor)];

/// The Jump Pads, working once the Power is on: from the pit floor up over the Rail Yard's rail,
/// and from the foot of the Headframe up onto its deck, past its Power Door.
const JUMP_PADS: [JumpPad; 2] = [
    JumpPad { zone: ZoneId::QuarryFloor, at: at(6.0, 0.0, -3.0), to: at(6.0, TERRACE_1, 7.0) },
    JumpPad { zone: ZoneId::Headframe, at: at(1.5, TERRACE_2, 30.0), to: at(-1.0, DECK, 37.5) },
];

/// The lamps.
const LAMPS: [(ZoneId, Vec3); 8] = [
    (ZoneId::PumpHouse, at(0.0, STOREY - 0.5, -25.0)),
    (ZoneId::QuarryFloor, at(-6.0, STOREY + 2.5, -9.0)),
    (ZoneId::QuarryFloor, at(12.0, STOREY + 2.5, -12.0)),
    (ZoneId::RailYard, at(9.0, TERRACE_1 + STOREY + 1.0, 9.0)),
    (ZoneId::CrusherShed, at(-10.0, TERRACE_1 + STOREY - 0.5, 9.0)),
    (ZoneId::WindingHouse, at(10.0, TERRACE_2 + STOREY - 0.5, 22.0)),
    (ZoneId::Offices, at(-10.0, TERRACE_2 + STOREY - 0.5, 22.0)),
    (ZoneId::Headframe, at(1.5, DECK + STOREY - 0.5, 32.0)),
];

/// The sump: a pond gone dry in the pit floor's south-west corner, a few steps down.
const SUMP: Sunken = Sunken { zone: ZoneId::QuarryFloor, min: v(-16.0, -16.0), max: v(-8.0, -8.0), depth: 0.6, rim: true };

/// Gravel, for the ramp; pale stone, for the sump's steps; and black iron, for the Headframe's
/// stairs.
const GRAVEL: Color = Color::srgb(0.4, 0.37, 0.31);
const SUMP_STONE: Color = Color::srgb(0.56, 0.53, 0.46);
const IRON: Color = Color::srgb(0.12, 0.12, 0.13);

/// The ramp and the stairs between floors.
const SLOPES: [Slope; 5] = [
    // Up the pit's east side to the Rail Yard, a storey over eight metres.
    Slope { zone: ZoneId::QuarryFloor, foot: at(CLIMB_X, 0.0, -6.0), up: Vec2::Y, run: 8.0, rise: STOREY, width: 3.0, steps: false, colour: GRAVEL },
    // Up from the Rail Yard to the Winding House's Door.
    Slope { zone: ZoneId::RailYard, foot: at(CLIMB_X, TERRACE_1, 8.0), up: Vec2::Y, run: 8.0, rise: STOREY, width: 3.0, steps: true, colour: IRON },
    // Up the Headframe's west side to its deck, behind the Power Door.
    Slope { zone: ZoneId::Headframe, foot: at(STAIR_X, TERRACE_2, 30.0), up: Vec2::Y, run: 8.0, rise: STOREY, width: 3.0, steps: true, colour: IRON },
    // Down into the sump on its east and north sides.
    Slope { zone: ZoneId::QuarryFloor, foot: at(-9.2, -SUMP.depth, -12.0), up: Vec2::X, run: 1.2, rise: SUMP.depth, width: 3.0, steps: true, colour: SUMP_STONE },
    Slope { zone: ZoneId::QuarryFloor, foot: at(-12.0, -SUMP.depth, -9.2), up: Vec2::Y, run: 1.2, rise: SUMP.depth, width: 3.0, steps: true, colour: SUMP_STONE },
];

/// The Props. The pit floor keeps one island to run laps round, an old excavator's carcass.
const PROPS: &[Prop] = {
    use PropKind::*;
    use ZoneId::*;
    &[
        // The Pump House: a bench along its east wall.
        prop(Bench, PumpHouse, (Q + 7.55, 0.0, -22.0), (0.5, 0.5, 2.4)),
        // The Quarry Floor: the excavator, rubble at the foot of the walls, and fuel drums.
        prop(Machine, QuarryFloor, (Q + 2.0, 0.0, -11.0), (4.0, 2.4, 3.0)),
        prop(Rubble, QuarryFloor, (Q - 18.8, 0.0, -18.6), (1.4, 0.85, 1.4)),
        prop(Rubble, QuarryFloor, (Q + 18.8, 0.0, -18.8), (1.4, 0.7, 1.2)),
        prop(Barrel, QuarryFloor, (Q - 17.0, 0.0, 1.2), (0.65, 0.95, 0.65)),
        prop(Barrel, QuarryFloor, (Q - 17.8, 0.0, 0.9), (0.65, 0.95, 0.65)),
        // The Rail Yard: an ore tub left on its rails, and a stack of sleepers.
        prop(Machine, RailYard, (Q + 10.0, TERRACE_1, 13.8), (2.4, 1.6, 1.4)),
        prop(Crate, RailYard, (Q + 1.2, TERRACE_1, 15.2), (1.2, 0.7, 1.2)),
        // The Crusher Shed: the crusher, an island to run round, with a rack and drums.
        prop(Machine, CrusherShed, (Q - 10.0, TERRACE_1, 9.0), (5.0, 2.6, 4.0)),
        prop(Shelf, CrusherShed, (Q - 15.0, TERRACE_1, 2.5), (3.0, 2.2, 0.6)),
        prop(Barrel, CrusherShed, (Q - 18.6, TERRACE_1, 15.2), (0.65, 0.95, 0.65)),
        // The Winding House: the winding drum, and a crate of spares.
        prop(Machine, WindingHouse, (Q + 10.0, TERRACE_2, 22.0), (5.0, 2.4, 3.0)),
        prop(Crate, WindingHouse, (Q + 1.5, TERRACE_2, 17.0), (1.0, 0.7, 1.0)),
        // The Offices: a big plan table, and filing cabinets.
        prop(Table, Offices, (Q - 10.0, TERRACE_2, 22.0), (4.0, 0.76, 1.4)),
        prop(Shelf, Offices, (Q - 19.4, TERRACE_2, 18.5), (0.6, 1.8, 1.6)),
        // The Headframe: the legs holding up the front of its deck.
        prop(Pillar, Headframe, (Q - 2.0, TERRACE_2, 34.3), (0.4, STOREY - 0.2, 0.4)),
        prop(Pillar, Headframe, (Q + 5.4, TERRACE_2, 34.3), (0.4, STOREY - 0.2, 0.4)),
    ]
};

/// The rock, the walls, the Doors, the rails, the ledges outside the windows and the Headframe.
fn build(b: &mut Builder) {
    use ZoneId::*;
    let rock = palette(Some(QuarryFloor)).wall;
    let outside = palette(None);

    // Ground and ledges of rock outside the windows, each as high as the floor its windows open
    // onto, with a parapet round any edge Zombies could fall from.
    b.block(Vec3::new(24.0, 0.2, 4.0), at(0.0, -0.1, -32.0), outside.floor);
    b.block(Vec3::new(4.0, 0.2, 10.0), at(-10.0, -0.1, -25.0), outside.floor);
    b.block(Vec3::new(4.0, 0.2, 8.0), at(-22.0, -0.1, -4.0), outside.floor);
    b.block(Vec3::new(4.0, 0.2, 8.0), at(22.0, -0.1, -14.0), outside.floor);
    shelf(b, v(20.0, 2.0), v(24.0, 9.0), TERRACE_1, rock);
    shelf(b, v(-24.0, 2.0), v(-20.0, 16.0), TERRACE_1, rock);
    shelf(b, v(20.0, 16.0), v(24.0, 26.0), TERRACE_2, rock);
    shelf(b, v(-24.0, 16.0), v(-20.0, 26.0), TERRACE_2, rock);
    shelf(b, v(6.0, 28.0), v(20.0, 32.0), TERRACE_2, rock);
    shelf(b, v(-20.0, 28.0), v(-6.0, 32.0), TERRACE_2, rock);

    // The Pump House, with the Trap's Door out to the pit.
    b.outer_wall(v(-8.0, -30.0), v(8.0, -30.0), 0.0, STOREY);
    b.outer_wall(v(-8.0, -30.0), v(-8.0, -20.0), 0.0, STOREY);
    b.outer_wall(v(8.0, -30.0), v(8.0, -20.0), 0.0, STOREY);
    b.wall(v(-8.0, -20.0), v(8.0, -20.0), 0.0, STOREY, &[(8.0, door(PumpHouse, QuarryFloor, DOOR_PRICE_PUMP_FLOOR))]);

    // The pit's rock walls, two storeys high, with an old adit planked up in each side.
    let cliff = 2.0 * STOREY;
    b.outer_wall(v(-20.0, -20.0), v(-20.0, 2.0), 0.0, cliff);
    b.outer_wall(v(20.0, -20.0), v(20.0, 2.0), 0.0, cliff);
    b.wall(v(-20.0, -20.0), v(-8.0, -20.0), 0.0, cliff, &[]);
    b.wall(v(8.0, -20.0), v(20.0, -20.0), 0.0, cliff, &[]);
    // The face of the first terrace, and its edge: a solid wall either side of the Door at the top
    // of the ramp, and a rail along the rest with two gaps to drop through.
    b.wall(v(-20.0, 2.0), v(20.0, 2.0), 0.0, TERRACE_1, &[]);
    b.wall(v(12.0, 2.0), v(20.0, 2.0), TERRACE_1, STOREY, &[(CLIMB_X - 12.0, door(QuarryFloor, RailYard, DOOR_PRICE_FLOOR_RAIL_YARD))]);
    let drop = || Gap::Open { width: DOOR_WIDTH, height: RAIL_HEIGHT };
    b.wall(v(0.0, 2.0), v(12.0, 2.0), TERRACE_1, RAIL_HEIGHT, &[(3.0, drop()), (9.0, drop())]);

    // The first terrace: the Rail Yard's east wall, the Crusher Shed round it, and the face of the
    // second terrace behind them both.
    b.outer_wall(v(20.0, 2.0), v(20.0, 16.0), TERRACE_1, STOREY);
    b.outer_wall(v(-20.0, 2.0), v(-20.0, 16.0), TERRACE_1, STOREY);
    b.wall(v(-20.0, 2.0), v(0.0, 2.0), TERRACE_1, STOREY, &[]);
    b.wall(v(0.0, 2.0), v(0.0, 16.0), TERRACE_1, STOREY, &[(7.0, door(RailYard, CrusherShed, DOOR_PRICE_RAIL_YARD_CRUSHER))]);
    b.wall(v(-20.0, 16.0), v(20.0, 16.0), TERRACE_1, STOREY, &[]);

    // The second terrace: the Winding House and the Offices side by side.
    b.wall(v(-20.0, 16.0), v(0.0, 16.0), TERRACE_2, STOREY, &[]);
    b.wall(v(0.0, 16.0), v(20.0, 16.0), TERRACE_2, STOREY, &[(CLIMB_X, door(RailYard, WindingHouse, DOOR_PRICE_RAIL_YARD_WINDING))]);
    b.outer_wall(v(20.0, 16.0), v(20.0, 28.0), TERRACE_2, STOREY);
    b.outer_wall(v(-20.0, 16.0), v(-20.0, 28.0), TERRACE_2, STOREY);
    b.wall(v(0.0, 16.0), v(0.0, 28.0), TERRACE_2, STOREY, &[(6.0, door(WindingHouse, Offices, DOOR_PRICE_WINDING_OFFICES))]);
    b.wall(v(0.0, 28.0), v(20.0, 28.0), TERRACE_2, STOREY, &[(3.0, door(WindingHouse, Headframe, DOOR_PRICE_WINDING_HEADFRAME)), (14.0, Gap::Window)]);
    b.outer_wall(v(-20.0, 28.0), v(0.0, 28.0), TERRACE_2, STOREY);

    // The Headframe: steel walls two storeys high round its yard, a parapet over the roofs to its
    // south, the stairs walled off from the yard and shut behind a Power Door at their foot, and
    // the deck over the back of the yard, railed along its front with a gap to drop through.
    let girder = palette(Some(Headframe)).wall;
    b.wall(v(-6.0, 28.0), v(-6.0, 40.0), TERRACE_2, 2.0 * STOREY, &[]);
    b.wall(v(6.0, 28.0), v(6.0, 40.0), TERRACE_2, 2.0 * STOREY, &[]);
    b.wall(v(-6.0, 40.0), v(6.0, 40.0), TERRACE_2, 2.0 * STOREY, &[]);
    b.wall(v(-6.0, 28.0), v(6.0, 28.0), DECK, STOREY, &[]);
    b.wall(v(-6.0, 30.0), v(-2.6, 30.0), TERRACE_2, DOOR_HEIGHT, &[(STAIR_X + 6.0, Gap::Door(Some((Headframe, Headframe)), None))]);
    b.wall(v(-2.6, 30.0), v(-2.6, 38.0), TERRACE_2, STOREY + RAIL_HEIGHT, &[]);
    // The space under the stairs' upper half is boxed in solid, so nothing is ever lost in it.
    b.block(Vec3::new(3.0, 1.3, 5.0), at(STAIR_X, TERRACE_2 + 0.65, 35.5), girder);
    let deck = palette(Some(Headframe)).floor;
    b.block(Vec3::new(8.2, 0.2, 5.8), at(1.7, DECK - 0.1, 36.9), deck);
    b.block(Vec3::new(3.4, 0.2, 1.8), at(-4.1, DECK - 0.1, 38.9), deck);
    b.wall(v(-2.6, 34.0), v(6.0, 34.0), DECK, RAIL_HEIGHT, &[(7.1, Gap::Open { width: DOOR_WIDTH, height: RAIL_HEIGHT })]);
    // The winding wheel at the top of the tower, against the sky.
    b.block(Vec3::new(0.3, 3.6, 3.6), at(0.0, DECK + STOREY + 2.0, 39.0), girder);
}

/// A ledge of rock outside a terrace's windows, `min`..`max` across and its top at `top`, with a
/// parapet along each edge that isn't against the building.
fn shelf(b: &mut Builder, min: Vec2, max: Vec2, top: f32, rock: Color) {
    let size = max - min;
    let mid = (min + max) / 2.0;
    b.block(Vec3::new(size.x, top, size.y), Vec3::new(mid.x, top / 2.0, mid.y), palette(None).floor);
    let thick = PARAPET_THICKNESS;
    let rise = top + RAIL_HEIGHT / 2.0;
    // Which side the building is on: the one nearer the Quarry's middle.
    let building_x = if (mid.x - Q).abs() > 19.0 { Some(if mid.x > Q { min.x } else { max.x }) } else { None };
    for x in [min.x + thick / 2.0, max.x - thick / 2.0] {
        if building_x.is_some_and(|wall| (wall - x).abs() < 1.0) {
            continue;
        }
        b.block(Vec3::new(thick, RAIL_HEIGHT, size.y), Vec3::new(x, rise, mid.y), rock);
    }
    for z in [min.y + thick / 2.0, max.y - thick / 2.0] {
        // The north ledges back onto the buildings to their south.
        if building_x.is_none() && z < mid.y {
            continue;
        }
        b.block(Vec3::new(size.x, RAIL_HEIGHT, thick), Vec3::new(mid.x, rise, z), rock);
    }
}

/// The path nodes: a name, then where a Zombie's feet stand there (x, y, z).
const NODES: &[(&str, f32, f32, f32)] = &[
    // The Pump House
    ("pump", Q, 0.0, -25.5),
    ("pump_n", Q, 0.0, -21.5),
    ("w0_in", Q - 4.0, 0.0, -28.5),
    ("w0_out", Q - 4.0, 0.0, -32.0),
    ("w1_in", Q + 4.0, 0.0, -28.5),
    ("w1_out", Q + 4.0, 0.0, -32.0),
    ("w2_in", Q - 6.5, 0.0, -25.0),
    ("w2_out", Q - 10.0, 0.0, -25.0),
    // The Quarry Floor, round the excavator
    ("floor_s", Q, 0.0, -18.3),
    ("floor_sw", Q - 14.0, 0.0, -18.0),
    ("isl_s", Q + 2.0, 0.0, -15.0),
    ("isl_w", Q - 2.0, 0.0, -11.0),
    ("isl_e", Q + 6.5, 0.0, -11.0),
    ("isl_n", Q + 2.0, 0.0, -7.0),
    // Down in the sump, and on the floor at the top of each flight of its steps
    ("sump", Q - 12.5, -0.6, -12.5),
    ("sump_e", Q - 7.2, 0.0, -12.0),
    ("sump_n", Q - 12.0, 0.0, -7.2),
    ("floor_w", Q - 15.0, 0.0, -3.0),
    ("floor_nw", Q - 8.0, 0.0, -1.0),
    // Under the two drops from the Rail Yard
    ("floor_n", Q + 3.0, 0.0, 0.5),
    ("floor_ne", Q + 9.0, 0.0, 0.5),
    ("floor_e", Q + 13.0, 0.0, -10.0),
    ("w3_in", Q - 18.5, 0.0, -4.0),
    ("w3_out", Q - 22.0, 0.0, -4.0),
    ("w4_in", Q + 18.5, 0.0, -14.0),
    ("w4_out", Q + 22.0, 0.0, -14.0),
    // Up the ramp
    ("ramp_foot", Q + CLIMB_X, 0.0, -7.5),
    ("ramp_mid", Q + CLIMB_X, 2.0, -2.0),
    ("ramp_top", Q + CLIMB_X, 3.6, 1.2),
    // The Rail Yard
    ("yard_ramp", Q + CLIMB_X, TERRACE_1, 3.5),
    ("yard_dw", Q + 3.0, TERRACE_1, 3.2),
    ("yard_de", Q + 9.0, TERRACE_1, 3.2),
    ("yard_c", Q + 8.0, TERRACE_1, 8.0),
    ("yard_n", Q + 5.0, TERRACE_1, 11.0),
    ("yard_shed", Q + 1.5, TERRACE_1, 9.0),
    ("yard_steps", Q + CLIMB_X, TERRACE_1, 6.5),
    ("steps_mid", Q + CLIMB_X, TERRACE_1 + 2.0, 12.0),
    ("steps_top", Q + CLIMB_X, TERRACE_1 + 3.6, 15.2),
    ("w5_in", Q + 18.5, TERRACE_1, 4.5),
    ("w5_out", Q + 22.0, TERRACE_1, 4.5),
    // The Crusher Shed, round the crusher
    ("shed_e", Q - 1.5, TERRACE_1, 9.0),
    ("shed_se", Q - 4.0, TERRACE_1, 4.5),
    ("shed_s", Q - 10.0, TERRACE_1, 4.5),
    ("shed_sw", Q - 16.0, TERRACE_1, 4.5),
    ("shed_w", Q - 16.0, TERRACE_1, 9.0),
    ("shed_nw", Q - 16.0, TERRACE_1, 13.5),
    ("shed_n", Q - 10.0, TERRACE_1, 13.5),
    ("shed_ne", Q - 4.0, TERRACE_1, 13.5),
    ("w6_in", Q - 18.5, TERRACE_1, 5.0),
    ("w6_out", Q - 22.0, TERRACE_1, 5.0),
    ("w7_in", Q - 18.5, TERRACE_1, 12.0),
    ("w7_out", Q - 22.0, TERRACE_1, 12.0),
    // The Winding House, round the drum
    ("wind_s", Q + CLIMB_X, TERRACE_2, 17.5),
    ("wind_se", Q + 15.0, TERRACE_2, 19.0),
    ("wind_sw", Q + 4.0, TERRACE_2, 19.0),
    ("wind_w", Q + 1.5, TERRACE_2, 22.0),
    ("wind_n", Q + 3.0, TERRACE_2, 26.3),
    ("wind_ne", Q + 15.0, TERRACE_2, 25.5),
    ("w8_in", Q + 18.5, TERRACE_2, 22.0),
    ("w8_out", Q + 22.0, TERRACE_2, 22.0),
    ("w9_in", Q + 14.0, TERRACE_2, 26.5),
    ("w9_out", Q + 14.0, TERRACE_2, 30.0),
    // The Offices, round the plan table
    ("off_e", Q - 1.5, TERRACE_2, 22.0),
    ("off_s", Q - 6.0, TERRACE_2, 18.0),
    ("off_sw", Q - 15.0, TERRACE_2, 18.5),
    ("off_n", Q - 6.0, TERRACE_2, 25.5),
    ("off_nw", Q - 14.0, TERRACE_2, 25.0),
    ("w10_in", Q - 18.5, TERRACE_2, 22.0),
    ("w10_out", Q - 22.0, TERRACE_2, 22.0),
    ("w11_in", Q - 12.0, TERRACE_2, 26.5),
    ("w11_out", Q - 12.0, TERRACE_2, 30.0),
    // The Headframe's yard, under its deck, and the corner by the Power Door
    ("head_s", Q + 3.0, TERRACE_2, 29.3),
    ("head_w", Q - 1.0, TERRACE_2, 31.5),
    ("head_e", Q + 5.0, TERRACE_2, 32.0),
    ("head_under", Q + 1.5, TERRACE_2, 37.0),
    ("head_corner", Q + STAIR_X, TERRACE_2, 28.9),
    // Up the stairs, and along the deck
    ("stair_low", Q + STAIR_X, TERRACE_2 + 0.4, 30.8),
    ("stair_mid", Q + STAIR_X, TERRACE_2 + 2.0, 34.0),
    ("stair_top", Q + STAIR_X, TERRACE_2 + 3.8, 37.6),
    ("landing", Q + STAIR_X, DECK, 39.0),
    ("deck_w", Q - 1.0, DECK, 38.8),
    ("deck_c", Q + 1.5, DECK, 37.5),
    ("deck_e", Q + 4.6, DECK, 36.5),
    // Either side of the gap in the deck's rail
    ("deck_drop", Q + 4.5, DECK, 34.8),
    ("head_drop", Q + 4.5, TERRACE_2, 32.8),
    // Vents, up near the top of their rooms, over the spot a Nova Crawler drops to
    ("vent_shed", Q - 10.0, TERRACE_1 + NOVA_VENT_HEIGHT, 4.5),
    ("vent_wind", Q + 15.0, TERRACE_2 + NOVA_VENT_HEIGHT, 19.0),
    ("vent_off", Q - 14.0, TERRACE_2 + NOVA_VENT_HEIGHT, 25.0),
];

const OPEN: Gate = Gate::Open;

/// The edges, by node name. Barrier and one-way edges run from the first node to the second.
const EDGES: &[(&str, &str, Gate)] = {
    use ZoneId::*;
    &[
        // The Pump House
        ("pump", "pump_n", OPEN),
        ("pump", "w0_in", OPEN),
        ("pump", "w1_in", OPEN),
        ("pump", "w2_in", OPEN),
        ("w0_out", "w0_in", barrier(0)),
        ("w1_out", "w1_in", barrier(1)),
        ("w2_out", "w2_in", barrier(2)),
        ("pump_n", "floor_s", Gate::Door(PumpHouse, QuarryFloor)),
        // The Quarry Floor
        ("floor_s", "isl_s", OPEN),
        ("floor_s", "floor_sw", OPEN),
        ("floor_s", "isl_w", OPEN),
        ("isl_s", "isl_w", OPEN),
        ("isl_s", "isl_e", OPEN),
        ("isl_w", "isl_n", OPEN),
        ("isl_e", "isl_n", OPEN),
        ("isl_w", "sump_e", OPEN),
        ("sump_e", "sump", OPEN),
        ("sump", "sump_n", OPEN),
        ("sump_n", "floor_w", OPEN),
        ("sump_n", "floor_nw", OPEN),
        ("floor_w", "floor_nw", OPEN),
        ("floor_w", "w3_in", OPEN),
        ("w3_out", "w3_in", barrier(3)),
        ("floor_nw", "isl_n", OPEN),
        ("floor_nw", "floor_n", OPEN),
        ("floor_n", "floor_ne", OPEN),
        ("floor_n", "isl_n", OPEN),
        ("floor_ne", "isl_n", OPEN),
        ("floor_ne", "floor_e", OPEN),
        ("isl_e", "floor_e", OPEN),
        ("floor_e", "w4_in", OPEN),
        ("w4_out", "w4_in", barrier(4)),
        ("floor_e", "ramp_foot", OPEN),
        ("ramp_foot", "ramp_mid", OPEN),
        ("ramp_mid", "ramp_top", OPEN),
        ("ramp_top", "yard_ramp", Gate::Door(QuarryFloor, RailYard)),
        // The Rail Yard, and its drops into the pit
        ("yard_ramp", "yard_c", OPEN),
        ("yard_ramp", "w5_in", OPEN),
        ("yard_ramp", "yard_steps", OPEN),
        ("w5_out", "w5_in", barrier(5)),
        ("yard_c", "yard_dw", OPEN),
        ("yard_c", "yard_de", OPEN),
        ("yard_c", "yard_shed", OPEN),
        ("yard_c", "yard_n", OPEN),
        ("yard_c", "yard_steps", OPEN),
        ("yard_n", "yard_shed", OPEN),
        ("yard_dw", "floor_n", Gate::OneWay),
        ("yard_de", "floor_ne", Gate::OneWay),
        ("yard_shed", "shed_e", Gate::Door(RailYard, CrusherShed)),
        ("yard_steps", "steps_mid", OPEN),
        ("steps_mid", "steps_top", OPEN),
        ("steps_top", "wind_s", Gate::Door(RailYard, WindingHouse)),
        // The Crusher Shed
        ("shed_e", "shed_se", OPEN),
        ("shed_e", "shed_ne", OPEN),
        ("shed_se", "shed_s", OPEN),
        ("shed_s", "shed_sw", OPEN),
        ("shed_sw", "shed_w", OPEN),
        ("shed_w", "shed_nw", OPEN),
        ("shed_nw", "shed_n", OPEN),
        ("shed_n", "shed_ne", OPEN),
        ("shed_sw", "w6_in", OPEN),
        ("shed_nw", "w7_in", OPEN),
        ("w6_out", "w6_in", barrier(6)),
        ("w7_out", "w7_in", barrier(7)),
        // The Winding House
        ("wind_s", "wind_se", OPEN),
        ("wind_se", "wind_sw", OPEN),
        ("wind_se", "wind_ne", OPEN),
        ("wind_se", "w8_in", OPEN),
        ("wind_ne", "w8_in", OPEN),
        ("wind_ne", "w9_in", OPEN),
        ("wind_ne", "wind_n", OPEN),
        ("wind_sw", "wind_w", OPEN),
        ("wind_w", "wind_n", OPEN),
        ("w8_out", "w8_in", barrier(8)),
        ("w9_out", "w9_in", barrier(9)),
        ("wind_w", "off_e", Gate::Door(WindingHouse, Offices)),
        ("wind_n", "head_s", Gate::Door(WindingHouse, Headframe)),
        // The Offices
        ("off_e", "off_s", OPEN),
        ("off_e", "off_n", OPEN),
        ("off_s", "off_sw", OPEN),
        ("off_n", "off_nw", OPEN),
        ("off_sw", "off_nw", OPEN),
        ("off_sw", "w10_in", OPEN),
        ("off_nw", "w10_in", OPEN),
        ("off_nw", "w11_in", OPEN),
        ("w10_out", "w10_in", barrier(10)),
        ("w11_out", "w11_in", barrier(11)),
        // The Headframe
        ("head_s", "head_w", OPEN),
        ("head_s", "head_e", OPEN),
        ("head_w", "head_e", OPEN),
        ("head_w", "head_under", OPEN),
        ("head_e", "head_under", OPEN),
        ("head_s", "head_corner", OPEN),
        // Up the stairs, once the Power has opened their Door.
        ("head_corner", "stair_low", Gate::Door(Headframe, Headframe)),
        ("stair_low", "stair_mid", OPEN),
        ("stair_mid", "stair_top", OPEN),
        ("stair_top", "landing", OPEN),
        ("landing", "deck_w", OPEN),
        ("deck_w", "deck_c", OPEN),
        ("deck_c", "deck_e", OPEN),
        ("deck_e", "deck_drop", OPEN),
        // Off the deck's front.
        ("deck_drop", "head_drop", Gate::OneWay),
        ("head_drop", "head_e", OPEN),
        // The Vent drops
        ("vent_shed", "shed_s", Gate::OneWay),
        ("vent_wind", "wind_se", Gate::OneWay),
        ("vent_off", "off_nw", Gate::OneWay),
    ]
};
