//! The Quarry Map: an open-cast pit and the works built up its terraced side, climbed a storey
//! at a time. A Run starts in the Pump House at the foot of the pit and opens onto the Quarry
//! Floor, an open bowl with the sump pond in it and Zombies rising from the gravel. A ramp climbs
//! the east side to the Rail Yard on the first terrace, beside the Crusher Shed; steps climb from
//! there to the Winding House and the Offices on the second, and the Headframe stands over them,
//! with Pack-a-Punch up on its deck.
//!
//! Off the pit floor's west side, the Incline is a rock cutting that climbs past both terraces, a
//! landing at each with a Door into the Crusher Shed and the Offices, so either terrace can be
//! run round in a loop. Off its east side, the Loading Dock is a goods shed with wagons on the
//! ground and a loading platform. A spoil heap stands in the pit's south-east corner, up a ramp
//! and down a flight of steps.
//!
//! The deck has a window of its own, onto the lip of the pit behind the tower, so Zombies come in
//! up there as well as up the stairs.
//!
//! Going up costs Points and stairs; coming down is a drop off a Ledge, from the Rail Yard into
//! the pit, from one of the Incline's landings to the one below, or off the Headframe's deck,
//! which Zombies can follow but never climb back. Once the Power is on, Jump Pads throw a Player
//! back up: from the pit floor to the Rail Yard, and from the foot of the Headframe to its deck.
//! The Quarry has no Teleporter.
//!
//! The whole Map is centred on the world's origin along x (ADR 0009).

use bevy::prelude::*;

use crate::weapons::arsenal::WeaponId;
use crate::maps::jump_pad::JumpPad;
use crate::maps::map::BarrierId;
use crate::enemies::nav::Gate;
use crate::enemies::sapper::SapperTiming;
use crate::tuning::*;
use crate::maps::zone::ZoneId;

use super::build::{Builder, Gap};
use super::{DOOR_HEIGHT, DOOR_WIDTH, Infill, Layout, PARAPET_THICKNESS, Prop, PropKind, RAIL_HEIGHT, RaisedFloor, STOREY, Slope, Sunken, door, infill, palette, prop};

/// The first terrace's height, and the second's, and the Headframe's deck.
const TERRACE_1: f32 = STOREY;
const TERRACE_2: f32 = 2.0 * STOREY;
const DECK: f32 = 3.0 * STOREY;

/// Where the ramp up from the pit floor and the steps up from the Rail Yard run (x): both
/// against the Map's east wall, each with a Door at its top.
const CLIMB_X: f32 = 18.3;
/// Where the Headframe's stairs run up its west wall (x).
const STAIR_X: f32 = -4.3;
/// Where the Incline's ramp runs up its west wall, and its steps up its east wall (x).
const INCLINE_RAMP_X: f32 = -28.5;
const INCLINE_STEPS_X: f32 = -22.0;

/// The first of the Quarry's windows in `WINDOWS`, after the Greybox's.
const W0: u8 = super::greybox::WINDOWS.len() as u8;

/// A point in the Quarry, `x` along from its middle.
const fn at(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

/// A spot on a wall or a floor's plan in the Quarry, `x` along from its middle.
const fn v(x: f32, z: f32) -> Vec2 {
    Vec2::new(x, z)
}

const fn barrier(i: u8) -> Gate {
    Gate::Barrier(BarrierId(W0 + i))
}

pub(super) const QUARRY: Layout = Layout {
    zones: &{
        use ZoneId::*;
        [PumpHouse, QuarryFloor, RailYard, CrusherShed, WindingHouse, Offices, Headframe, Incline, LoadingDock]
    },
    // In the Pump House, clear of its Box, its windows and the Door out.
    seats: [at(0.0, 0.0, -26.0), at(-3.0, 0.0, -26.0), at(3.0, 0.0, -26.0), at(0.0, 0.0, -23.0)],
    wall_weapons: &WALL_WEAPONS,
    windows: &WINDOWS,
    risers: &RISERS,
    wading: &[],
    hellhound_spawns: &HELLHOUND_SPAWNS,
    special_spawns: &SPECIAL_SPAWNS,
    vents: &VENTS,
    raised_floors: &RAISED_FLOORS,
    infills: &INFILLS,
    slopes: &SLOPES,
    sunken: &[SUMP],
    water: &[],
    canals: &[],
    props: PROPS,
    lamps: &LAMPS,
    fixed_joins: &FIXED_JOINS,
    shifting: &[],
    wings: &[],
    fuel_lines: &[],
    floods: &[],
    nav_nodes: NODES,
    nav_edges: EDGES,
    jump_pads: &JUMP_PADS,
    ziplines: &[],
    sapper: SapperTiming::DEFAULT,
    sections: &[],
    generators: &[],
    darkness: false,
    outdoor: &[ZoneId::QuarryFloor, ZoneId::RailYard, ZoneId::Headframe, ZoneId::Incline],
    roofed: true,
    outside: Some(super::Outside { floor: super::textures::Material::GravelGround01, wall: super::textures::Material::RustyCorrugatedIron }),
    sun: Vec3::new(1.0, -3.0, -1.5),
    look: crate::maps::look::OVERCAST,
    lamp_style: super::LampStyle::Bulb,
    rail_style: super::RailStyle::Solid,
    bulb_zones: &[],
    plan: (Vec2::new(-36.0, -38.0), Vec2::new(38.0, 44.0)),
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

/// The Box Locations, in order. Every Run starts the Box at the first, at the foot of the
/// Incline, two Doors from the Pump House.
pub(super) const BOX_LOCATIONS: [(ZoneId, Vec3, Vec2); 6] = [
    // Against the south wall at the foot of the Incline.
    (ZoneId::Incline, at(-25.0, 0.0, -11.35), Vec2::Y),
    // Against the pit's south wall, west of the Pump House.
    (ZoneId::QuarryFloor, at(-17.5, 0.0, -19.35), Vec2::Y),
    // Against the Crusher Shed's south wall.
    (ZoneId::CrusherShed, at(-5.0, TERRACE_1, 2.65), Vec2::Y),
    // In the Offices' south-west corner.
    (ZoneId::Offices, at(-17.0, TERRACE_2, 16.65), Vec2::Y),
    // Under the Headframe's deck, against its east wall.
    (ZoneId::Headframe, at(5.35, TERRACE_2, 37.0), Vec2::NEG_X),
    // Up on the Loading Dock's platform, against its east wall.
    (ZoneId::LoadingDock, at(33.35, DOCK_PLATFORM.height, -11.0), Vec2::NEG_X),
];

/// The 14 windows: three in the Pump House, an old adit planked up in the pit's west wall, two
/// in the Loading Dock, one off each of the Incline's floors, the rest on the terraces, each
/// over a ledge of rock outside where Zombies gather, and one up on the Headframe's deck, onto
/// the lip of the pit behind it, so camping at Pack-a-Punch isn't free.
pub(super) const WINDOWS: [(ZoneId, Vec3, Vec2); 14] = [
    (ZoneId::PumpHouse, at(-4.0, 0.0, -30.0), Vec2::NEG_Y),
    (ZoneId::PumpHouse, at(4.0, 0.0, -30.0), Vec2::NEG_Y),
    (ZoneId::PumpHouse, at(-8.0, 0.0, -25.0), Vec2::NEG_X),
    (ZoneId::QuarryFloor, at(-20.0, 0.0, -16.0), Vec2::NEG_X),
    (ZoneId::LoadingDock, at(28.0, 0.0, -2.0), Vec2::Y),
    (ZoneId::RailYard, at(20.0, TERRACE_1, 4.5), Vec2::X),
    (ZoneId::Incline, at(-30.0, 0.0, -6.0), Vec2::NEG_X),
    (ZoneId::Incline, at(-30.0, TERRACE_1, 12.0), Vec2::NEG_X),
    (ZoneId::WindingHouse, at(20.0, TERRACE_2, 22.0), Vec2::X),
    (ZoneId::WindingHouse, at(14.0, TERRACE_2, 28.0), Vec2::Y),
    (ZoneId::Incline, at(-30.0, TERRACE_2, 27.0), Vec2::NEG_X),
    (ZoneId::Offices, at(-12.0, TERRACE_2, 28.0), Vec2::Y),
    (ZoneId::LoadingDock, at(27.0, 0.0, -20.0), Vec2::NEG_Y),
    (ZoneId::Headframe, at(DECK_WINDOW_X, DECK, 40.0), Vec2::Y),
];

/// Where along the Headframe's back wall its deck's window is, between the stairs' landing and
/// the Bowie Knife's chalk.
const DECK_WINDOW_X: f32 = 1.0;
/// The lip of the pit behind the Headframe, outside the deck's window: a ledge of rock at the
/// deck's height (x, z corners).
const PIT_LIP: (Vec2, Vec2) = (v(DECK_WINDOW_X - 3.0, 40.0), v(DECK_WINDOW_X + 3.0, 44.0));

/// The Risers: on the pit floor, clear of the sump, the spoil heap and the ramp, in the Rail
/// Yard, at the foot of the Incline and on the Loading Dock's floor.
const RISERS: [(ZoneId, Vec3); 8] = [
    (ZoneId::QuarryFloor, at(-4.0, 0.0, -15.0)),
    (ZoneId::QuarryFloor, at(15.5, 0.0, -8.5)),
    (ZoneId::QuarryFloor, at(-14.0, 0.0, -1.0)),
    (ZoneId::QuarryFloor, at(11.0, 0.0, -1.0)),
    (ZoneId::RailYard, at(4.0, TERRACE_1, 12.0)),
    (ZoneId::RailYard, at(12.0, TERRACE_1, 10.0)),
    (ZoneId::Incline, at(-24.0, 0.0, -2.0)),
    (ZoneId::LoadingDock, at(24.0, 0.0, -11.0)),
];

/// The Hellhound Spawn Points, two in every Zone, and one on each of the Incline's floors.
const HELLHOUND_SPAWNS: [(ZoneId, Vec3); 20] = [
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
    (ZoneId::Incline, at(-25.0, 0.0, -7.0)),
    (ZoneId::Incline, at(-26.5, TERRACE_1, 19.0)),
    (ZoneId::Incline, at(-26.0, TERRACE_2, 28.0)),
    (ZoneId::LoadingDock, at(24.0, 0.0, -11.0)),
    (ZoneId::LoadingDock, at(31.0, 0.0, -3.5)),
];

/// The Special Spawn Points, one in each of the bigger Zones.
const SPECIAL_SPAWNS: [(ZoneId, Vec3); 8] = [
    (ZoneId::QuarryFloor, at(-2.0, 0.0, -5.0)),
    (ZoneId::QuarryFloor, at(12.0, 0.0, -6.0)),
    (ZoneId::RailYard, at(12.0, TERRACE_1, 5.5)),
    (ZoneId::CrusherShed, at(-15.0, TERRACE_1, 6.0)),
    (ZoneId::WindingHouse, at(16.0, TERRACE_2, 25.0)),
    (ZoneId::Offices, at(-3.0, TERRACE_2, 18.5)),
    (ZoneId::Incline, at(-25.0, 0.0, -7.0)),
    (ZoneId::LoadingDock, at(24.0, 0.0, -11.0)),
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

/// The lamps: floodlights over the pit, the Rail Yard and the Incline, and under every ceiling
/// enough that no point of an indoor floor is further than `LAMP_REACH` from one in its Zone (a
/// test checks).
const LAMPS: [(ZoneId, Vec3); 25] = [
    (ZoneId::PumpHouse, at(-4.0, STOREY - 0.5, -25.0)),
    (ZoneId::PumpHouse, at(4.0, STOREY - 0.5, -25.0)),
    (ZoneId::QuarryFloor, at(-6.0, STOREY + 2.5, -9.0)),
    (ZoneId::QuarryFloor, at(12.0, STOREY + 2.5, -12.0)),
    (ZoneId::RailYard, at(9.0, TERRACE_1 + STOREY + 1.0, 9.0)),
    (ZoneId::CrusherShed, at(-15.0, TERRACE_1 + STOREY - 0.5, 6.0)),
    (ZoneId::CrusherShed, at(-15.0, TERRACE_1 + STOREY - 0.5, 12.0)),
    (ZoneId::CrusherShed, at(-5.0, TERRACE_1 + STOREY - 0.5, 6.0)),
    (ZoneId::CrusherShed, at(-5.0, TERRACE_1 + STOREY - 0.5, 12.0)),
    (ZoneId::WindingHouse, at(5.0, TERRACE_2 + STOREY - 0.5, 19.5)),
    (ZoneId::WindingHouse, at(5.0, TERRACE_2 + STOREY - 0.5, 24.5)),
    (ZoneId::WindingHouse, at(15.0, TERRACE_2 + STOREY - 0.5, 19.5)),
    (ZoneId::WindingHouse, at(15.0, TERRACE_2 + STOREY - 0.5, 24.5)),
    (ZoneId::Offices, at(-15.0, TERRACE_2 + STOREY - 0.5, 19.5)),
    (ZoneId::Offices, at(-15.0, TERRACE_2 + STOREY - 0.5, 24.5)),
    (ZoneId::Offices, at(-5.0, TERRACE_2 + STOREY - 0.5, 19.5)),
    (ZoneId::Offices, at(-5.0, TERRACE_2 + STOREY - 0.5, 24.5)),
    (ZoneId::Headframe, at(1.5, DECK + STOREY - 0.5, 32.0)),
    (ZoneId::Incline, at(-25.0, STOREY + 1.0, -5.0)),
    (ZoneId::Incline, at(-25.0, TERRACE_1 + STOREY - 0.5, 13.0)),
    (ZoneId::Incline, at(-25.0, TERRACE_2 + STOREY - 0.5, 27.0)),
    (ZoneId::LoadingDock, at(24.0, STOREY - 0.5, -16.0)),
    (ZoneId::LoadingDock, at(24.0, STOREY - 0.5, -6.0)),
    (ZoneId::LoadingDock, at(30.0, STOREY - 0.5, -16.0)),
    (ZoneId::LoadingDock, at(30.0, STOREY - 0.5, -6.0)),
];

/// The sump: a pond gone dry in the pit floor's south-west corner, a few steps down.
const SUMP: Sunken = Sunken { zone: ZoneId::QuarryFloor, min: v(-16.0, -16.0), max: v(-8.0, -8.0), depth: 0.6, rim: true };

/// Gravel, for the ramps and the spoil heap; pale stone, for the sump's steps; black iron, for
/// the Headframe's stairs; clay and pit timber, for the Incline's landings and steps; and
/// concrete, for the Loading Dock's platform.
const GRAVEL: Color = Color::srgb(0.4, 0.37, 0.31);
const SUMP_STONE: Color = Color::srgb(0.56, 0.53, 0.46);
const IRON: Color = Color::srgb(0.12, 0.12, 0.13);
const CLAY: Color = Color::srgb(0.34, 0.25, 0.17);
const TIMBER: Color = Color::srgb(0.47, 0.35, 0.22);
const CONCRETE: Color = Color::srgb(0.52, 0.51, 0.48);

/// The Incline's landings: the first terrace's, round the head of its ramp and on to the foot of
/// its steps, and the second terrace's at the top. The south edge of each drops a storey.
const INCLINE_LANDING: RaisedFloor = RaisedFloor { zone: ZoneId::Incline, min: v(-30.0, 8.0), max: v(-20.0, 16.0), height: TERRACE_1, colour: CLAY };
const INCLINE_LANDING_N: RaisedFloor = RaisedFloor { zone: ZoneId::Incline, min: v(-30.0, 16.0), max: v(-24.0, 24.0), height: TERRACE_1, colour: CLAY };
const INCLINE_TOP: RaisedFloor = RaisedFloor { zone: ZoneId::Incline, min: v(-30.0, 24.0), max: v(-20.0, 30.0), height: TERRACE_2, colour: CLAY };
/// The Loading Dock's platform along its east wall, wagon-bed high: too high to step onto, so
/// it's the steps up.
const DOCK_PLATFORM: RaisedFloor = RaisedFloor { zone: ZoneId::LoadingDock, min: v(30.0, -17.0), max: v(34.0, -5.0), height: 1.1, colour: CONCRETE };
/// The spoil heap in the pit's south-east corner: up a ramp from the west, down steps to the north.
const SPOIL_HEAP: RaisedFloor = RaisedFloor { zone: ZoneId::QuarryFloor, min: v(9.0, -19.8), max: v(15.0, -15.0), height: 1.2, colour: GRAVEL };

const RAISED_FLOORS: [RaisedFloor; 5] = [INCLINE_LANDING, INCLINE_LANDING_N, INCLINE_TOP, DOCK_PLATFORM, SPOIL_HEAP];

/// The Infills, one to three in each indoor Zone, so no two rooms share a shape. Each keeps clear
/// of the Doors, windows, Buyables and Spawn Points, and of every path-node edge, and leaves the
/// loop round each room's island wider than 3 m.
const INFILLS: [Infill; 12] = {
    use ZoneId::*;
    [
        // The Pump House: a chimney breast in the south-east corner beside the bench, and a switch
        // cupboard boxed off in the north-west corner.
        infill(PumpHouse, (6.6, -30.0), (8.0, -28.6)),
        infill(PumpHouse, (-8.0, -21.8), (-6.8, -20.0)),
        // The Crusher Shed: a flue breast in the middle of its north wall, and a boxed-off corner
        // in the north-east.
        infill(CrusherShed, (-11.0, 14.8), (-9.0, 16.0)),
        infill(CrusherShed, (-2.2, 14.2), (0.0, 16.0)),
        // The Winding House: a pier on the south wall, a nib on the north wall between the Door and
        // the window, and a drive cupboard in the north-east corner.
        infill(WindingHouse, (12.8, 16.0), (14.4, 17.4)),
        infill(WindingHouse, (8.2, 27.0), (10.0, 28.0)),
        infill(WindingHouse, (18.0, 26.0), (20.0, 28.0)),
        // The Offices: a chimney breast on the north wall, and a cupboard in the south-east corner.
        infill(Offices, (-16.8, 26.8), (-14.8, 28.0)),
        infill(Offices, (-1.4, 16.0), (0.0, 17.6)),
        // The Loading Dock: a corner either side of the platform at its east wall, and a pier on
        // the south wall past the window.
        infill(LoadingDock, (32.8, -3.4), (34.0, -2.0)),
        infill(LoadingDock, (32.8, -20.0), (34.0, -18.6)),
        infill(LoadingDock, (30.0, -20.0), (31.4, -19.2)),
    ]
};

/// The ramps and the stairs between floors.
const SLOPES: [Slope; 10] = [
    // Up the pit's east side to the Rail Yard, a storey over eight metres.
    Slope { zone: ZoneId::QuarryFloor, foot: at(CLIMB_X, 0.0, -6.0), up: Vec2::Y, run: 8.0, rise: STOREY, width: 3.0, steps: false, colour: GRAVEL },
    // Up from the Rail Yard to the Winding House's Door.
    Slope { zone: ZoneId::RailYard, foot: at(CLIMB_X, TERRACE_1, 8.0), up: Vec2::Y, run: 8.0, rise: STOREY, width: 3.0, steps: true, colour: IRON },
    // Up the Headframe's west side to its deck, behind the Power Door.
    Slope { zone: ZoneId::Headframe, foot: at(STAIR_X, TERRACE_2, 30.0), up: Vec2::Y, run: 8.0, rise: STOREY, width: 3.0, steps: true, colour: IRON },
    // Down into the sump on its east and north sides.
    Slope { zone: ZoneId::QuarryFloor, foot: at(-9.2, -SUMP.depth, -12.0), up: Vec2::X, run: 1.2, rise: SUMP.depth, width: 3.0, steps: true, colour: SUMP_STONE },
    Slope { zone: ZoneId::QuarryFloor, foot: at(-12.0, -SUMP.depth, -9.2), up: Vec2::Y, run: 1.2, rise: SUMP.depth, width: 3.0, steps: true, colour: SUMP_STONE },
    // Up onto the spoil heap from the west, and down off it to the north.
    Slope { zone: ZoneId::QuarryFloor, foot: at(5.5, 0.0, -17.4), up: Vec2::X, run: 3.5, rise: SPOIL_HEAP.height, width: 3.0, steps: false, colour: GRAVEL },
    Slope { zone: ZoneId::QuarryFloor, foot: at(12.0, 0.0, -13.2), up: Vec2::NEG_Y, run: 1.8, rise: SPOIL_HEAP.height, width: 3.0, steps: true, colour: SUMP_STONE },
    // Up the Incline's west wall to its first landing, and up its east wall to the top.
    Slope { zone: ZoneId::Incline, foot: at(INCLINE_RAMP_X, 0.0, 0.0), up: Vec2::Y, run: 8.0, rise: TERRACE_1, width: 3.0, steps: false, colour: GRAVEL },
    Slope { zone: ZoneId::Incline, foot: at(INCLINE_STEPS_X, TERRACE_1, 16.0), up: Vec2::Y, run: 8.0, rise: STOREY, width: 4.0, steps: true, colour: TIMBER },
    // Up onto the Loading Dock's platform.
    Slope { zone: ZoneId::LoadingDock, foot: at(28.0, 0.0, -11.0), up: Vec2::X, run: 2.0, rise: DOCK_PLATFORM.height, width: 3.0, steps: true, colour: CONCRETE },
];

/// The Props. The pit floor keeps one island to run laps round, an old excavator's carcass.
const PROPS: &[Prop] = {
    use PropKind::*;
    use ZoneId::*;
    &[
        // The Pump House: a bench along its east wall.
        prop(Bench, PumpHouse, (7.55, 0.0, -22.0), (0.5, 0.5, 2.4)),
        // The Quarry Floor: the excavator, rubble at the foot of the walls, and fuel drums.
        prop(Machine, QuarryFloor, (2.0, 0.0, -11.0), (4.0, 2.4, 3.0)),
        prop(Rubble, QuarryFloor, (18.8, 0.0, -18.8), (1.4, 0.7, 1.2)),
        prop(Barrel, QuarryFloor, (-17.0, 0.0, 1.2), (0.65, 0.95, 0.65)),
        prop(Barrel, QuarryFloor, (-17.8, 0.0, 0.9), (0.65, 0.95, 0.65)),
        // The Rail Yard: an ore tub left on its rails, and a stack of sleepers.
        prop(Machine, RailYard, (10.0, TERRACE_1, 13.8), (2.4, 1.6, 1.4)),
        prop(Crate, RailYard, (1.2, TERRACE_1, 15.2), (1.2, 0.7, 1.2)),
        // The Crusher Shed: the crusher, an island to run round, with a rack and drums.
        prop(Machine, CrusherShed, (-10.0, TERRACE_1, 9.0), (5.0, 2.6, 4.0)),
        prop(Shelf, CrusherShed, (-15.0, TERRACE_1, 2.5), (3.0, 2.2, 0.6)),
        prop(Barrel, CrusherShed, (-18.6, TERRACE_1, 15.2), (0.65, 0.95, 0.65)),
        // The Winding House: the winding drum, and a crate of spares.
        prop(Machine, WindingHouse, (10.0, TERRACE_2, 22.0), (5.0, 2.4, 3.0)),
        prop(Crate, WindingHouse, (1.5, TERRACE_2, 17.0), (1.0, 0.7, 1.0)),
        // The Offices: a big plan table, and filing cabinets.
        prop(Table, Offices, (-10.0, TERRACE_2, 22.0), (4.0, 0.76, 1.4)),
        prop(Shelf, Offices, (-19.4, TERRACE_2, 18.5), (0.6, 1.8, 1.6)),
        // The Incline: an ore tub run off its rails at the foot, and drums up at the top.
        prop(Machine, Incline, (-21.2, 0.0, 2.5), (1.4, 1.2, 2.0)),
        prop(Barrel, Incline, (-29.0, 0.0, -10.8), (0.65, 0.95, 0.65)),
        prop(Barrel, Incline, (-21.0, TERRACE_2, 29.2), (0.65, 0.95, 0.65)),
        // The Loading Dock: two wagons in from the sidings, and a crate up on the platform.
        prop(Machine, LoadingDock, (25.75, 0.0, -6.5), (3.5, 1.5, 2.0)),
        prop(Machine, LoadingDock, (25.75, 0.0, -15.5), (3.5, 1.5, 2.0)),
        prop(Crate, LoadingDock, (33.0, 1.1, -15.8), (1.0, 0.8, 1.0)),
        // The Headframe: the legs holding up the front of its deck.
        prop(Pillar, Headframe, (-2.0, TERRACE_2, 34.3), (0.4, STOREY - 0.2, 0.4)),
        prop(Pillar, Headframe, (5.4, TERRACE_2, 34.3), (0.4, STOREY - 0.2, 0.4)),
    ]
};

/// The rock, the walls, the Doors, the rails, the ledges outside the windows and the Headframe.
fn build(b: &mut Builder) {
    use ZoneId::*;
    let rock = palette(Some(QuarryFloor)).wall;

    // Ground and ledges of rock outside the windows, each as high as the floor its windows open
    // onto, with a parapet round any edge Zombies could fall from.
    b.ground(Vec3::new(24.0, 0.2, 4.0), at(0.0, -0.1, -32.0));
    b.ground(Vec3::new(4.0, 0.2, 10.0), at(-10.0, -0.1, -25.0));
    b.ground(Vec3::new(4.0, 0.2, 8.0), at(-22.0, -0.1, -16.0));
    b.ground(Vec3::new(4.0, 0.2, 8.0), at(-32.0, -0.1, -6.0));
    b.ground(Vec3::new(8.0, 0.2, 4.0), at(28.0, -0.1, 0.0));
    b.ground(Vec3::new(8.0, 0.2, 4.0), at(27.0, -0.1, -22.0));
    shelf(b, v(20.0, 2.0), v(24.0, 9.0), TERRACE_1, rock);
    shelf(b, v(-34.0, 8.0), v(-30.0, 16.0), TERRACE_1, rock);
    shelf(b, v(20.0, 16.0), v(24.0, 26.0), TERRACE_2, rock);
    shelf(b, v(-34.0, 23.0), v(-30.0, 30.0), TERRACE_2, rock);
    shelf(b, v(6.0, 28.0), v(20.0, 32.0), TERRACE_2, rock);
    shelf(b, v(-20.0, 28.0), v(-6.0, 32.0), TERRACE_2, rock);
    shelf(b, PIT_LIP.0, PIT_LIP.1, DECK, rock);

    // The Pump House, with the Trap's Door out to the pit.
    b.outer_wall(v(-8.0, -30.0), v(8.0, -30.0), 0.0, STOREY);
    b.outer_wall(v(-8.0, -30.0), v(-8.0, -20.0), 0.0, STOREY);
    b.outer_wall(v(8.0, -30.0), v(8.0, -20.0), 0.0, STOREY);
    b.wall(v(-8.0, -20.0), v(8.0, -20.0), 0.0, STOREY, &[(8.0, door(PumpHouse, QuarryFloor, DOOR_PRICE_PUMP_FLOOR))]);

    // The pit's rock walls, two storeys high, with an old adit planked up in the west one, the
    // Door into the Incline beyond it, and the Door into the Loading Dock in the east one.
    let cliff = 2.0 * STOREY;
    b.outer_wall(v(-20.0, -20.0), v(-20.0, -12.0), 0.0, cliff);
    b.wall(v(-20.0, -12.0), v(-20.0, 2.0), 0.0, cliff, &[(4.0, door(QuarryFloor, Incline, DOOR_PRICE_FLOOR_INCLINE))]);
    b.wall(v(20.0, -20.0), v(20.0, -2.0), 0.0, cliff, &[(6.0, door(QuarryFloor, LoadingDock, DOOR_PRICE_FLOOR_DOCK))]);
    b.outer_wall(v(20.0, -2.0), v(20.0, 2.0), 0.0, cliff);
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
    // Two storeys high, so the iron steps beside it top out 4 m under its top: nothing can climb
    // onto it and off the far side.
    b.outer_wall(v(20.0, 2.0), v(20.0, 16.0), TERRACE_1, 2.0 * STOREY);
    b.wall(v(-20.0, 2.0), v(-20.0, 16.0), 0.0, TERRACE_1, &[]);
    b.wall(v(-20.0, 2.0), v(-20.0, 16.0), TERRACE_1, STOREY, &[(10.0, door(Incline, CrusherShed, DOOR_PRICE_INCLINE_CRUSHER))]);
    b.wall(v(-20.0, 2.0), v(0.0, 2.0), TERRACE_1, STOREY, &[]);
    b.wall(v(0.0, 2.0), v(0.0, 16.0), TERRACE_1, STOREY, &[(7.0, door(RailYard, CrusherShed, DOOR_PRICE_RAIL_YARD_CRUSHER))]);
    b.wall(v(-20.0, 16.0), v(20.0, 16.0), TERRACE_1, STOREY, &[]);

    // The second terrace: the Winding House and the Offices side by side.
    b.wall(v(-20.0, 16.0), v(0.0, 16.0), TERRACE_2, STOREY, &[]);
    b.wall(v(0.0, 16.0), v(20.0, 16.0), TERRACE_2, STOREY, &[(CLIMB_X, door(RailYard, WindingHouse, DOOR_PRICE_RAIL_YARD_WINDING))]);
    b.outer_wall(v(20.0, 16.0), v(20.0, 28.0), TERRACE_2, STOREY);
    b.wall(v(-20.0, 16.0), v(-20.0, 28.0), 0.0, TERRACE_2, &[]);
    b.wall(v(-20.0, 16.0), v(-20.0, 28.0), TERRACE_2, STOREY, &[(10.0, door(Incline, Offices, DOOR_PRICE_INCLINE_OFFICES))]);
    b.wall(v(-20.0, 28.0), v(-20.0, 30.0), TERRACE_2, STOREY, &[]);
    b.wall(v(0.0, 16.0), v(0.0, 28.0), TERRACE_2, STOREY, &[(6.0, door(WindingHouse, Offices, DOOR_PRICE_WINDING_OFFICES))]);
    b.wall(v(0.0, 28.0), v(20.0, 28.0), TERRACE_2, STOREY, &[(3.0, door(WindingHouse, Headframe, DOOR_PRICE_WINDING_HEADFRAME)), (14.0, Gap::Window)]);
    b.outer_wall(v(-20.0, 28.0), v(0.0, 28.0), TERRACE_2, STOREY);

    // The Incline: a cutting walled by the cliff to the south and west, a window off each of its
    // floors, with a rail along the front of each landing and a gap in it to drop through. The
    // space under its steps is boxed in solid, so nothing is ever lost in it.
    b.outer_wall(v(-30.0, -12.0), v(-20.0, -12.0), 0.0, cliff);
    b.outer_wall(v(-30.0, -12.0), v(-30.0, 30.0), 0.0, TERRACE_1);
    b.outer_wall(v(-30.0, -12.0), v(-30.0, 30.0), TERRACE_1, STOREY);
    b.outer_wall(v(-30.0, 16.0), v(-30.0, 30.0), TERRACE_2, STOREY);
    b.outer_wall(v(-30.0, 30.0), v(-20.0, 30.0), 0.0, TERRACE_2 + STOREY);
    b.wall(v(INCLINE_RAMP_X + 1.5, 8.0), v(-20.0, 8.0), TERRACE_1, RAIL_HEIGHT, &[(3.5, drop())]);
    b.wall(v(-30.0, 24.0), v(-24.0, 24.0), TERRACE_2, RAIL_HEIGHT, &[(3.0, drop())]);
    b.block(Vec3::new(4.0, TERRACE_1, 8.0), at(INCLINE_STEPS_X, TERRACE_1 / 2.0, 20.0), palette(Some(Incline)).wall);

    // The Loading Dock: a goods shed of one storey off the pit's east side, a window to the
    // sidings north and south.
    b.outer_wall(v(20.0, -20.0), v(34.0, -20.0), 0.0, STOREY);
    b.outer_wall(v(34.0, -20.0), v(34.0, -2.0), 0.0, STOREY);
    b.outer_wall(v(20.0, -2.0), v(34.0, -2.0), 0.0, STOREY);

    // The Headframe: steel walls two storeys high round its yard, a parapet over the roofs to its
    // south, the stairs walled off from the yard and shut behind a Power Door at their foot, and
    // the deck over the back of the yard, railed along its front with a gap to drop through.
    let girder = palette(Some(Headframe)).wall;
    b.wall(v(-6.0, 28.0), v(-6.0, 40.0), TERRACE_2, 2.0 * STOREY, &[]);
    b.wall(v(6.0, 28.0), v(6.0, 40.0), TERRACE_2, 2.0 * STOREY, &[]);
    // The back wall, with the deck's window up in its top storey.
    b.wall(v(-6.0, 40.0), v(6.0, 40.0), TERRACE_2, STOREY, &[]);
    b.outer_wall(v(-6.0, 40.0), v(6.0, 40.0), DECK, STOREY);
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
/// parapet along each edge that isn't against the building. A Player can clear a parapet, so an
/// invisible wall a storey higher stops them going over; Zombies pass it.
fn shelf(b: &mut Builder, min: Vec2, max: Vec2, top: f32, rock: Color) {
    let size = max - min;
    let mid = (min + max) / 2.0;
    b.ground(Vec3::new(size.x, top, size.y), Vec3::new(mid.x, top / 2.0, mid.y));
    let thick = PARAPET_THICKNESS;
    let rise = top + RAIL_HEIGHT / 2.0;
    // Which side the building is on: the one nearer the Quarry's middle.
    let building_x = if mid.x.abs() > 19.0 { Some(if mid.x > 0.0 { min.x } else { max.x }) } else { None };
    for x in [min.x + thick / 2.0, max.x - thick / 2.0] {
        if building_x.is_some_and(|wall| (wall - x).abs() < 1.0) {
            continue;
        }
        b.block(Vec3::new(thick, RAIL_HEIGHT, size.y), Vec3::new(x, rise, mid.y), rock);
        b.player_wall(Vec3::new(thick, STOREY, size.y), Vec3::new(x, top + RAIL_HEIGHT + STOREY / 2.0, mid.y));
    }
    for z in [min.y + thick / 2.0, max.y - thick / 2.0] {
        // The north ledges back onto the buildings to their south.
        if building_x.is_none() && z < mid.y {
            continue;
        }
        b.block(Vec3::new(size.x, RAIL_HEIGHT, thick), Vec3::new(mid.x, rise, z), rock);
        b.player_wall(Vec3::new(size.x, STOREY, thick), Vec3::new(mid.x, top + RAIL_HEIGHT + STOREY / 2.0, z));
    }
}

/// The path nodes: a name, then where a Zombie's feet stand there (x, y, z).
const NODES: &[(&str, f32, f32, f32)] = &[
    // The Pump House
    ("pump", 0.0, 0.0, -25.5),
    ("pump_n", 0.0, 0.0, -21.5),
    ("w0_in", -4.0, 0.0, -28.5),
    ("w0_out", -4.0, 0.0, -32.0),
    ("w1_in", 4.0, 0.0, -28.5),
    ("w1_out", 4.0, 0.0, -32.0),
    ("w2_in", -6.5, 0.0, -25.0),
    ("w2_out", -10.0, 0.0, -25.0),
    // The Quarry Floor, round the excavator
    ("floor_s", 0.0, 0.0, -18.3),
    ("floor_sw", -14.0, 0.0, -18.0),
    ("isl_s", 2.0, 0.0, -15.0),
    ("isl_w", -2.0, 0.0, -11.0),
    ("isl_e", 6.5, 0.0, -11.0),
    ("isl_n", 2.0, 0.0, -7.0),
    // Down in the sump, and on the floor at the top of each flight of its steps
    ("sump", -12.5, -0.6, -12.5),
    ("sump_e", -7.2, 0.0, -12.0),
    ("sump_n", -12.0, 0.0, -7.2),
    ("floor_w", -15.0, 0.0, -3.0),
    ("floor_nw", -8.0, 0.0, -1.0),
    // Under the two drops from the Rail Yard
    ("floor_n", 3.0, 0.0, 0.5),
    ("floor_ne", 9.0, 0.0, 0.5),
    ("floor_e", 13.0, 0.0, -10.0),
    ("w3_in", -18.5, 0.0, -16.0),
    ("w3_out", -22.0, 0.0, -16.0),
    // By the Doors into the Incline and the Loading Dock
    ("floor_wdoor", -18.5, 0.0, -8.0),
    ("floor_edoor", 18.5, 0.0, -14.0),
    // Up the spoil heap's ramp, across its top, and down its steps
    ("heap_foot", 4.0, 0.0, -17.4),
    ("heap_w", 9.8, SPOIL_HEAP.height, -17.4),
    ("heap_c", 12.0, SPOIL_HEAP.height, -17.4),
    ("heap_n", 12.0, SPOIL_HEAP.height, -15.8),
    ("heap_steps", 12.0, 0.0, -12.4),
    // Up the ramp
    ("ramp_foot", CLIMB_X, 0.0, -7.5),
    ("ramp_mid", CLIMB_X, 2.0, -2.0),
    ("ramp_top", CLIMB_X, 3.6, 1.2),
    // The Rail Yard
    ("yard_ramp", CLIMB_X, TERRACE_1, 3.5),
    ("yard_dw", 3.0, TERRACE_1, 3.2),
    ("yard_de", 9.0, TERRACE_1, 3.2),
    ("yard_c", 8.0, TERRACE_1, 8.0),
    ("yard_n", 5.0, TERRACE_1, 11.0),
    ("yard_shed", 1.5, TERRACE_1, 9.0),
    ("yard_steps", CLIMB_X, TERRACE_1, 6.5),
    ("steps_mid", CLIMB_X, TERRACE_1 + 2.0, 12.0),
    ("steps_top", CLIMB_X, TERRACE_1 + 3.6, 15.2),
    ("w5_in", 18.5, TERRACE_1, 4.5),
    ("w5_out", 22.0, TERRACE_1, 4.5),
    // The Crusher Shed, round the crusher
    ("shed_e", -1.5, TERRACE_1, 9.0),
    ("shed_se", -4.0, TERRACE_1, 4.5),
    ("shed_s", -10.0, TERRACE_1, 4.5),
    ("shed_sw", -16.0, TERRACE_1, 4.5),
    ("shed_w", -16.0, TERRACE_1, 9.0),
    ("shed_nw", -16.0, TERRACE_1, 13.5),
    ("shed_n", -10.0, TERRACE_1, 13.5),
    ("shed_ne", -4.0, TERRACE_1, 13.5),
    ("shed_wdoor", -18.5, TERRACE_1, 12.0),
    // The Winding House, round the drum
    ("wind_s", CLIMB_X, TERRACE_2, 17.5),
    ("wind_se", 15.0, TERRACE_2, 19.0),
    ("wind_sw", 4.0, TERRACE_2, 19.0),
    ("wind_w", 1.5, TERRACE_2, 22.0),
    ("wind_n", 3.0, TERRACE_2, 26.3),
    ("wind_ne", 15.0, TERRACE_2, 25.5),
    ("w8_in", 18.5, TERRACE_2, 22.0),
    ("w8_out", 22.0, TERRACE_2, 22.0),
    ("w9_in", 14.0, TERRACE_2, 26.5),
    ("w9_out", 14.0, TERRACE_2, 30.0),
    // The Offices, round the plan table
    ("off_e", -1.5, TERRACE_2, 22.0),
    ("off_s", -6.0, TERRACE_2, 18.0),
    ("off_sw", -15.0, TERRACE_2, 18.5),
    ("off_n", -6.0, TERRACE_2, 25.5),
    ("off_nw", -14.0, TERRACE_2, 25.0),
    ("off_wdoor", -18.5, TERRACE_2, 26.0),
    ("w11_in", -12.0, TERRACE_2, 26.5),
    ("w11_out", -12.0, TERRACE_2, 30.0),
    // The Headframe's yard, under its deck, and the corner by the Power Door
    ("head_s", 3.0, TERRACE_2, 29.3),
    ("head_w", -1.0, TERRACE_2, 31.5),
    ("head_e", 5.0, TERRACE_2, 32.0),
    ("head_under", 1.5, TERRACE_2, 37.0),
    ("head_corner", STAIR_X, TERRACE_2, 28.9),
    // Up the stairs, and along the deck
    ("stair_low", STAIR_X, TERRACE_2 + 0.4, 30.8),
    ("stair_mid", STAIR_X, TERRACE_2 + 2.0, 34.0),
    ("stair_top", STAIR_X, TERRACE_2 + 3.8, 37.6),
    ("landing", STAIR_X, DECK, 39.0),
    ("deck_w", -1.0, DECK, 38.8),
    ("deck_c", 1.5, DECK, 37.5),
    ("deck_e", 4.6, DECK, 36.5),
    ("w13_in", DECK_WINDOW_X, DECK, 38.5),
    ("w13_out", DECK_WINDOW_X, DECK, 42.0),
    // Either side of the gap in the deck's rail
    ("deck_drop", 4.5, DECK, 34.8),
    ("head_drop", 4.5, TERRACE_2, 32.8),
    // The foot of the Incline, round its gallery and up its ramp
    ("inc_door", -21.5, 0.0, -8.0),
    ("inc_s", -25.0, 0.0, -8.0),
    ("inc_c", -24.5, 0.0, -3.0),
    ("inc_low_n", -23.5, 0.0, 6.5),
    ("w6_in", -28.5, 0.0, -6.0),
    ("w6_out", -32.0, 0.0, -6.0),
    ("inc_ramp_foot", INCLINE_RAMP_X, 0.0, -1.5),
    ("inc_ramp_mid", INCLINE_RAMP_X, 2.0, 4.0),
    ("inc_ramp_top", INCLINE_RAMP_X, 3.6, 7.2),
    // The Incline's first landing, by the Crusher Shed's Door, and up its steps
    ("inc_land", -27.0, TERRACE_1, 10.0),
    ("inc_drop", -23.5, TERRACE_1, 9.2),
    ("inc_shed", -21.5, TERRACE_1, 12.0),
    ("w7_in", -28.5, TERRACE_1, 12.0),
    ("w7_out", -32.0, TERRACE_1, 12.0),
    ("inc_land_n", -27.0, TERRACE_1, 19.0),
    ("inc_land_drop", -27.0, TERRACE_1, 22.5),
    ("inc_steps_foot", INCLINE_STEPS_X, TERRACE_1, 14.5),
    ("inc_steps_mid", INCLINE_STEPS_X, TERRACE_1 + 2.0, 20.0),
    ("inc_steps_top", INCLINE_STEPS_X, TERRACE_1 + 3.6, 23.2),
    // The Incline's top landing, by the Offices' Door
    ("inc_off", -21.5, TERRACE_2, 26.0),
    ("inc_top", -25.0, TERRACE_2, 27.5),
    ("inc_top_drop", -27.0, TERRACE_2, 25.2),
    ("w10_in", -28.5, TERRACE_2, 27.0),
    ("w10_out", -32.0, TERRACE_2, 27.0),
    // The Loading Dock, round its wagons, and up on its platform
    ("dock_w", 21.8, 0.0, -14.0),
    ("dock_nw", 21.8, 0.0, -4.0),
    ("dock_sw", 21.8, 0.0, -18.2),
    ("dock_c", 26.0, 0.0, -11.0),
    ("dock_ne", 29.0, 0.0, -6.5),
    ("dock_se", 29.0, 0.0, -15.5),
    ("w4_in", 28.0, 0.0, -3.6),
    ("w4_out", 28.0, 0.0, 0.0),
    ("w12_in", 27.0, 0.0, -18.4),
    ("w12_out", 27.0, 0.0, -22.0),
    ("plat_c", 31.0, DOCK_PLATFORM.height, -11.0),
    ("plat_n", 31.2, DOCK_PLATFORM.height, -6.5),
    ("plat_s", 31.2, DOCK_PLATFORM.height, -15.0),
    // Vents, up near the top of their rooms, over the spot a Nova Crawler drops to
    ("vent_shed", -10.0, TERRACE_1 + NOVA_VENT_HEIGHT, 4.5),
    ("vent_wind", 15.0, TERRACE_2 + NOVA_VENT_HEIGHT, 19.0),
    ("vent_off", -14.0, TERRACE_2 + NOVA_VENT_HEIGHT, 25.0),
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
        ("floor_w", "floor_wdoor", OPEN),
        ("floor_wdoor", "w3_in", OPEN),
        ("floor_sw", "w3_in", OPEN),
        ("w3_out", "w3_in", barrier(3)),
        ("floor_wdoor", "inc_door", Gate::Door(QuarryFloor, Incline)),
        ("floor_nw", "isl_n", OPEN),
        ("floor_nw", "floor_n", OPEN),
        ("floor_n", "floor_ne", OPEN),
        ("floor_n", "isl_n", OPEN),
        ("floor_ne", "isl_n", OPEN),
        ("floor_ne", "floor_e", OPEN),
        ("isl_e", "floor_e", OPEN),
        ("floor_e", "floor_edoor", OPEN),
        ("floor_edoor", "dock_w", Gate::Door(QuarryFloor, LoadingDock)),
        // The spoil heap
        ("floor_s", "heap_foot", OPEN),
        ("isl_s", "heap_foot", OPEN),
        ("heap_foot", "heap_w", OPEN),
        ("heap_w", "heap_c", OPEN),
        ("heap_w", "heap_n", OPEN),
        ("heap_c", "heap_n", OPEN),
        ("heap_n", "heap_steps", OPEN),
        ("heap_steps", "isl_e", OPEN),
        ("heap_steps", "floor_e", OPEN),
        ("heap_steps", "floor_edoor", OPEN),
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
        ("shed_w", "shed_wdoor", OPEN),
        ("shed_nw", "shed_wdoor", OPEN),
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
        ("off_sw", "off_wdoor", OPEN),
        ("off_nw", "off_wdoor", OPEN),
        ("off_nw", "w11_in", OPEN),
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
        ("w13_in", "deck_w", OPEN),
        ("w13_in", "deck_c", OPEN),
        ("w13_out", "w13_in", barrier(13)),
        // Off the deck's front.
        ("deck_drop", "head_drop", Gate::OneWay),
        ("head_drop", "head_e", OPEN),
        // The foot of the Incline
        ("inc_door", "inc_s", OPEN),
        ("inc_door", "inc_c", OPEN),
        ("inc_s", "inc_c", OPEN),
        ("inc_s", "w6_in", OPEN),
        ("w6_out", "w6_in", barrier(6)),
        ("w6_in", "inc_ramp_foot", OPEN),
        ("inc_c", "inc_ramp_foot", OPEN),
        ("inc_c", "inc_low_n", OPEN),
        ("inc_ramp_foot", "inc_ramp_mid", OPEN),
        ("inc_ramp_mid", "inc_ramp_top", OPEN),
        ("inc_ramp_top", "inc_land", OPEN),
        // Its first landing, and the drop off it
        ("inc_land", "inc_drop", OPEN),
        ("inc_land", "inc_shed", OPEN),
        ("inc_drop", "inc_shed", OPEN),
        ("inc_land", "w7_in", OPEN),
        ("w7_out", "w7_in", barrier(7)),
        ("inc_land", "inc_land_n", OPEN),
        ("inc_land_n", "inc_land_drop", OPEN),
        ("inc_shed", "inc_steps_foot", OPEN),
        ("inc_drop", "inc_low_n", Gate::OneWay),
        ("inc_shed", "shed_wdoor", Gate::Door(Incline, CrusherShed)),
        // Up its steps to the top landing, and the drop off that
        ("inc_steps_foot", "inc_steps_mid", OPEN),
        ("inc_steps_mid", "inc_steps_top", OPEN),
        ("inc_steps_top", "inc_off", OPEN),
        ("inc_steps_top", "inc_top", OPEN),
        ("inc_off", "inc_top", OPEN),
        ("inc_top", "inc_top_drop", OPEN),
        ("inc_top", "w10_in", OPEN),
        ("w10_out", "w10_in", barrier(10)),
        ("inc_top_drop", "inc_land_drop", Gate::OneWay),
        ("inc_off", "off_wdoor", Gate::Door(Incline, Offices)),
        // The Loading Dock
        ("dock_w", "dock_nw", OPEN),
        ("dock_w", "dock_sw", OPEN),
        ("dock_w", "dock_c", OPEN),
        ("dock_nw", "w4_in", OPEN),
        ("dock_sw", "w12_in", OPEN),
        ("dock_c", "dock_ne", OPEN),
        ("dock_c", "dock_se", OPEN),
        ("dock_ne", "w4_in", OPEN),
        ("dock_se", "w12_in", OPEN),
        ("w4_out", "w4_in", barrier(4)),
        ("w12_out", "w12_in", barrier(12)),
        ("dock_c", "plat_c", OPEN),
        ("plat_c", "plat_n", OPEN),
        ("plat_c", "plat_s", OPEN),
        ("plat_n", "dock_ne", Gate::OneWay),
        ("plat_s", "dock_se", Gate::OneWay),
        // The Vent drops
        ("vent_shed", "shed_s", Gate::OneWay),
        ("vent_wind", "wind_se", Gate::OneWay),
        ("vent_off", "off_nw", Gate::OneWay),
    ]
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maps::map::MapId;

    /// The deck's window, onto the lip of the pit.
    const DECK_WINDOW: BarrierId = BarrierId(W0 + 13);

    /// Pack-a-Punch's deck isn't a free camp: a window lets Zombies onto it from the lip of the
    /// pit, besides the stairs.
    #[test]
    fn the_headframe_deck_has_a_window_of_its_own() {
        let (zone, centre, _) = WINDOWS[13];
        assert_eq!(zone, ZoneId::Headframe);
        assert_eq!(centre.y, DECK);
        assert_eq!(crate::maps::map::WINDOWS[DECK_WINDOW.0 as usize].1, centre);
    }

    /// The lip of the pit outside the deck's window is walled in all round, so a Zombie knocked
    /// about up there never falls off the edge of the Map.
    #[test]
    fn the_lip_outside_the_deck_window_is_walled_in() {
        let open = crate::maps::map::tests::ways_off_outside(MapId::Quarry, DECK_WINDOW);
        assert!(open.is_empty(), "ways off the lip at these headings (degrees): {open:?}");
    }
}
